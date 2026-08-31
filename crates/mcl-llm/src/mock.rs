use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use futures::stream;

use crate::backend::{ChatMessage, LlmBackend, LlmStream};
use mcl_spec::MclError;

pub struct MockLlmBackend {
    responses: Vec<String>,
    cursor: AtomicUsize,
}

impl MockLlmBackend {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            cursor: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmBackend for MockLlmBackend {
    async fn complete(
        &self,
        _system: &str,
        _history: &[ChatMessage],
        _user_message: &str,
    ) -> Result<String, MclError> {
        let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
        self.responses
            .get(idx)
            .cloned()
            .ok_or_else(|| MclError::Llm("no more mock responses".to_string()))
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "mock"
    }

    fn stream(&self, _system: &str, _history: &[ChatMessage], _user_message: &str) -> LlmStream {
        let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
        let response = match self.responses.get(idx) {
            Some(r) => r.clone(),
            None => {
                return Box::pin(stream::once(async move {
                    Err(MclError::Llm("no more mock responses".to_string()))
                }));
            }
        };

        let chunks: Vec<String> = response.split(' ').map(|s| s.to_string()).collect();

        Box::pin(stream::iter(chunks.into_iter().map(Ok)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_responses_sequentially() {
        let backend = MockLlmBackend::new(vec!["first".to_string(), "second".to_string()]);

        assert_eq!(backend.complete("", &[], "hi").await.unwrap(), "first");
        assert_eq!(backend.complete("", &[], "hi").await.unwrap(), "second");
    }

    #[tokio::test]
    async fn exhausted_returns_error() {
        let backend = MockLlmBackend::new(vec!["only one".to_string()]);

        assert_eq!(backend.complete("", &[], "hi").await.unwrap(), "only one");
        let err = backend.complete("", &[], "hi").await.unwrap_err();
        assert!(matches!(err, MclError::Llm(msg) if msg == "no more mock responses"));
    }

    #[test]
    fn name_is_mock() {
        let backend = MockLlmBackend::new(vec![]);
        assert_eq!(backend.name(), "mock");
    }

    #[tokio::test]
    async fn streaming_yields_chunks_in_order() {
        let backend = MockLlmBackend::new(vec!["hello world test".to_string()]);
        let mut stream = backend.stream("", &[], "hi");

        use futures::StreamExt;
        let chunks: Vec<String> = stream
            .by_ref()
            .map(|r| r.expect("chunk should be Ok"))
            .collect()
            .await;

        assert_eq!(chunks, vec!["hello", "world", "test"]);
    }

    #[tokio::test]
    async fn streaming_reassembles_to_original() {
        let backend = MockLlmBackend::new(vec!["the quick brown fox".to_string()]);
        let stream = backend.stream("", &[], "hi");

        use futures::StreamExt;
        let reassembled: String = stream
            .map(|r| r.expect("chunk should be Ok"))
            .collect::<Vec<_>>()
            .await
            .join(" ");

        assert_eq!(reassembled, "the quick brown fox");
    }

    #[tokio::test]
    async fn streaming_exhausted_returns_error() {
        let backend = MockLlmBackend::new(vec!["only one".to_string()]);

        use futures::StreamExt;
        let stream1 = backend.stream("", &[], "hi");
        let _: Vec<_> = stream1
            .map(|r| r.expect("first stream should have chunks"))
            .collect()
            .await;

        let mut stream2 = backend.stream("", &[], "hi");
        let result = stream2.next().await;
        assert!(matches!(
            result,
            Some(Err(MclError::Llm(msg))) if msg == "no more mock responses"
        ));
    }
}
