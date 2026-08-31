use async_trait::async_trait;
use futures::StreamExt;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

use crate::backend::{ChatMessage, ChatRole, LlmBackend, LlmStream};
use crate::normalize::normalize_response;
use crate::retry::with_retry;

/// OpenAI-compatible backend. Works with OpenAI, OpenRouter, vLLM, etc.
pub struct OpenAiBackend {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
    response_format: Option<serde_json::Value>,
}

impl OpenAiBackend {
    pub fn new(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: model.into(),
            api_key: api_key.into(),
            response_format: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_response_format(mut self, format: serde_json::Value) -> Self {
        self.response_format = Some(format);
        self
    }
}

pub fn mcl_response_schema(actions: &[&str]) -> serde_json::Value {
    let _ = actions; // Available for future per-action schema refinement
    serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "mcl_response",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "narration": { "type": "string", "description": "Narrative text for the player" },
                    "signals": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string", "enum": ["action", "dialogue", "transition", "noop", "mutate"] },
                                "name": { "type": "string" },
                                "args": { "type": "object" },
                                "entity": { "type": "string" },
                                "text": { "type": "string" },
                                "context": { "type": "string" }
                            },
                            "required": ["type"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["narration", "signals"],
                "additionalProperties": false
            }
        }
    })
}

#[derive(Serialize, Clone)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmBackend for OpenAiBackend {
    async fn complete(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Result<String, MclError> {
        crate::backend::warn_if_unknown_model(self.name(), &self.model);

        let mut messages = Vec::new();

        messages.push(OpenAiMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });

        for msg in history {
            messages.push(OpenAiMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }

        messages.push(OpenAiMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages,
            stream: None,
            response_format: self.response_format.clone(),
        };

        let url = format!("{}/chat/completions", self.base_url);
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
                    .header("Authorization", format!("Bearer {api_key}"))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| MclError::Llm(format!("openai request failed: {e}")))
            }
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MclError::Llm(format!("openai returned {status}: {body}")));
        }

        let api_resp: OpenAiResponse = resp
            .json()
            .await
            .map_err(|e| MclError::Llm(format!("openai response parse error: {e}")))?;

        api_resp
            .choices
            .first()
            .map(|c| normalize_response(&c.message.content))
            .ok_or_else(|| MclError::Llm("openai returned no choices".to_string()))
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream(&self, system: &str, history: &[ChatMessage], user_message: &str) -> LlmStream {
        let client = self.client.clone();
        let url = format!("{}/chat/completions", self.base_url);
        let api_key = self.api_key.clone();

        let mut messages = Vec::new();
        messages.push(OpenAiMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });
        for msg in history {
            messages.push(OpenAiMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }
        messages.push(OpenAiMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = OpenAiRequest {
            model: self.model.clone(),
            messages,
            stream: Some(true),
            response_format: self.response_format.clone(),
        };

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let resp = match client
                .post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Err(MclError::Llm(format!("openai stream request: {e}"))))
                        .await;
                    return;
                }
            };
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(Err(MclError::Llm(format!(
                        "openai stream returned {status}: {body}"
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
                            // SSE format: lines prefixed with "data: "
                            let data = if let Some(stripped) = line.strip_prefix("data: ") {
                                stripped
                            } else {
                                continue;
                            };
                            if data == "[DONE]" {
                                return;
                            }
                            match serde_json::from_str::<serde_json::Value>(data) {
                                Ok(parsed) => {
                                    if let Some(content) =
                                        parsed["choices"][0]["delta"]["content"].as_str()
                                    {
                                        if !content.is_empty()
                                            && tx.send(Ok(content.to_string())).await.is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(MclError::Llm(format!(
                                            "openai stream parse: {e}"
                                        ))))
                                        .await;
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(MclError::Llm(format!("openai stream read: {e}"))))
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

    #[test]
    fn response_format_schema_is_valid_json() {
        let schema = mcl_response_schema(&["attack", "look", "pickup"]);
        assert!(schema["json_schema"]["schema"]["properties"]["narration"].is_object());
        assert!(schema["json_schema"]["schema"]["properties"]["signals"].is_object());
    }

    #[tokio::test]
    async fn openai_http_error_returns_llm_error() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("429"),
            "error should mention status code: {err}"
        );
    }

    #[tokio::test]
    async fn openai_empty_choices_returns_llm_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"choices": []}"#))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no choices"),
            "error should mention no choices: {err}"
        );
    }

    #[tokio::test]
    async fn openai_retries_on_server_error_then_succeeds() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"choices": [{"message": {"role": "assistant", "content": "hi back"}}]}"#,
            ))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hi back");
    }

    #[tokio::test]
    async fn openai_client_error_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "client error should not be retried");
    }

    #[tokio::test]
    async fn openai_retries_exhausted() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "should make exactly 3 retry attempts");
    }

    #[tokio::test]
    async fn openai_400_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "400 should not be retried");
    }

    #[tokio::test]
    async fn openai_403_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "403 should not be retried");
    }
}
