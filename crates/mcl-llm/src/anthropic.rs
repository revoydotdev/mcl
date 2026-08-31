use async_trait::async_trait;
use futures::StreamExt;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

use crate::backend::{ChatMessage, ChatRole, LlmBackend, LlmStream};
use crate::normalize::normalize_response;
use crate::retry::with_retry;

pub struct AnthropicBackend {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
    max_tokens: u32,
}

impl AnthropicBackend {
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.anthropic.com".to_string(),
            model: model.into(),
            api_key: api_key.into(),
            max_tokens: 4096,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }
}

#[derive(Serialize, Clone)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "user", // system handled separately
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmBackend for AnthropicBackend {
    async fn complete(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Result<String, MclError> {
        crate::backend::warn_if_unknown_model(self.name(), &self.model);

        let mut messages = Vec::new();

        // Anthropic uses a separate system field, not a system message
        for msg in history {
            if msg.role == ChatRole::System {
                continue; // skip system messages in history
            }
            messages.push(AnthropicMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }

        messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: system.to_string(),
            messages,
            stream: None,
        };

        let url = format!("{}/v1/messages", self.base_url);
        let client = self.client.clone();
        let api_key = self.api_key.clone();

        let resp = with_retry(|_| {
            let client = client.clone();
            let url = url.clone();
            let api_key = api_key.clone();
            let request = request.clone();
            async move {
                client
                    .post(&url)
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| MclError::Llm(format!("anthropic request failed: {e}")))
            }
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MclError::Llm(format!(
                "anthropic returned {status}: {body}"
            )));
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| MclError::Llm(format!("anthropic response parse error: {e}")))?;

        api_resp
            .content
            .first()
            .map(|c| normalize_response(&c.text))
            .ok_or_else(|| MclError::Llm("anthropic returned no content".to_string()))
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream(&self, system: &str, history: &[ChatMessage], user_message: &str) -> LlmStream {
        let client = self.client.clone();
        let url = format!("{}/v1/messages", self.base_url);
        let api_key = self.api_key.clone();

        let mut messages = Vec::new();
        for msg in history {
            if msg.role == ChatRole::System {
                continue;
            }
            messages.push(AnthropicMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }
        messages.push(AnthropicMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system: system.to_string(),
            messages,
            stream: Some(true),
        };

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let resp = match client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Err(MclError::Llm(format!("anthropic stream request: {e}"))))
                        .await;
                    return;
                }
            };
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(Err(MclError::Llm(format!(
                        "anthropic stream returned {status}: {body}"
                    ))))
                    .await;
                return;
            }

            let mut bytes = resp.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = bytes.next().await {
                match chunk {
                    Ok(b) => {
                        buffer.push_str(&String::from_utf8_lossy(&b));
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer = buffer[pos + 1..].to_string();
                            if line.is_empty() {
                                continue;
                            }
                            // SSE: event lines ("event: ...") and data lines ("data: ...")
                            if let Some(data) = line.strip_prefix("data: ") {
                                match serde_json::from_str::<serde_json::Value>(data) {
                                    Ok(parsed) => {
                                        let event_type = parsed["type"].as_str().unwrap_or("");
                                        if event_type == "message_stop" {
                                            return;
                                        }
                                        if event_type == "content_block_delta" {
                                            if let Some(text) = parsed["delta"]["text"].as_str() {
                                                if !text.is_empty()
                                                    && tx.send(Ok(text.to_string())).await.is_err()
                                                {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx
                                            .send(Err(MclError::Llm(format!(
                                                "anthropic stream parse: {e}"
                                            ))))
                                            .await;
                                        return;
                                    }
                                }
                            }
                            // "event: ..." lines are ignored; we key off type inside data
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(MclError::Llm(format!("anthropic stream read: {e}"))))
                            .await;
                        return;
                    }
                }
            }
        });

        Box::pin(ReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, matchers::path, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn anthropic_http_error_returns_llm_error() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(503).set_body_string(
                    r#"{"error": {"type": "overloaded", "message": "overloaded"}}"#,
                ),
            )
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("503"),
            "error should mention status code: {err}"
        );
    }

    #[tokio::test]
    async fn anthropic_empty_content_returns_llm_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"content": []}"#))
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no content"),
            "error should mention no content: {err}"
        );
    }

    #[tokio::test]
    async fn anthropic_retries_on_server_error_then_succeeds() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"content": [{"text": "hello from claude"}]}"#),
            )
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello from claude");
    }

    #[tokio::test]
    async fn anthropic_client_error_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "client error should not be retried");
    }

    #[tokio::test]
    async fn anthropic_retries_exhausted() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "should make exactly 3 retry attempts");
    }

    #[tokio::test]
    async fn anthropic_400_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "400 should not be retried");
    }

    #[tokio::test]
    async fn anthropic_403_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "403 should not be retried");
    }
}
