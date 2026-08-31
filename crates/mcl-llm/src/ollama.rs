use async_trait::async_trait;
use futures::StreamExt;
use mcl_spec::MclError;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

use crate::backend::{ChatMessage, ChatRole, LlmBackend, LlmStream};
use crate::normalize::normalize_response;
use crate::retry::with_retry;

pub struct OllamaBackend {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaBackend {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
            model: model.into(),
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[derive(Serialize, Clone)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: OllamaMessage,
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
    }
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(
        &self,
        system: &str,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Result<String, MclError> {
        crate::backend::warn_if_unknown_model(self.name(), &self.model);

        let mut messages = Vec::new();

        // System prompt
        messages.push(OllamaMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });

        // History
        for msg in history {
            messages.push(OllamaMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }

        // Current user message
        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
        };

        let url = format!("{}/api/chat", self.base_url);
        let client = self.client.clone();

        let resp = with_retry(|_| {
            let client = client.clone();
            let url = url.clone();
            let request = request.clone();
            async move {
                client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| MclError::Llm(format!("ollama request failed: {e}")))
            }
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(MclError::Llm(format!("ollama returned {status}: {body}")));
        }

        let chat_resp: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| MclError::Llm(format!("ollama response parse error: {e}")))?;

        Ok(normalize_response(&chat_resp.message.content))
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn stream(&self, system: &str, history: &[ChatMessage], user_message: &str) -> LlmStream {
        let client = self.client.clone();
        let url = format!("{}/api/chat", self.base_url);

        let mut messages = Vec::new();
        messages.push(OllamaMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });
        for msg in history {
            messages.push(OllamaMessage {
                role: role_str(msg.role).to_string(),
                content: msg.content.clone(),
            });
        }
        messages.push(OllamaMessage {
            role: "user".to_string(),
            content: user_message.to_string(),
        });

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let resp = match client.post(&url).json(&request).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Err(MclError::Llm(format!("ollama stream request: {e}"))))
                        .await;
                    return;
                }
            };
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let _ = tx
                    .send(Err(MclError::Llm(format!(
                        "ollama stream returned {status}: {body}"
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
                            match serde_json::from_str::<serde_json::Value>(&line) {
                                Ok(parsed) => {
                                    if let Some(content) = parsed["message"]["content"].as_str() {
                                        if !content.is_empty()
                                            && tx.send(Ok(content.to_string())).await.is_err()
                                        {
                                            return;
                                        }
                                    }
                                    if parsed["done"].as_bool() == Some(true) {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let _ = tx
                                        .send(Err(MclError::Llm(format!(
                                            "ollama stream parse: {e}"
                                        ))))
                                        .await;
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(MclError::Llm(format!("ollama stream read: {e}"))))
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
    async fn ollama_http_error_returns_llm_error() {
        // 500 is retryable — must pause time so backoff sleeps don't slow the test
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("500"),
            "error should mention status code: {err}"
        );
    }

    #[tokio::test]
    async fn ollama_malformed_response_returns_llm_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("parse error"),
            "error should mention parse failure: {err}"
        );
    }

    #[tokio::test]
    async fn ollama_retries_on_server_error() {
        tokio::time::pause();
        let server = MockServer::start().await;
        // Mount a 502 then a 200 — wiremock serves mocks in order
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(502))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"message": {"role": "assistant", "content": "hello"}}"#),
            )
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");
    }

    #[tokio::test]
    async fn ollama_client_error_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("401"),
            "error should mention status code: {err}"
        );

        // Client errors should not trigger retries — only 1 request made
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "client error should not be retried");
    }

    #[tokio::test]
    async fn ollama_retries_exhausted() {
        tokio::time::pause();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("503"),
            "error should mention status code: {err}"
        );

        // MAX_RETRIES = 3 attempts
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3, "should make exactly 3 retry attempts");
    }

    #[tokio::test]
    async fn ollama_400_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "400 should not be retried");
    }

    #[tokio::test]
    async fn ollama_403_fails_immediately() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let backend = OllamaBackend::new("test").with_url(server.uri());
        let result = backend.complete("sys", &[], "hi").await;
        assert!(result.is_err());

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "403 should not be retried");
    }
}
