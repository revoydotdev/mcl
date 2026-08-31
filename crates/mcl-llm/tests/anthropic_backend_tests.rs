use futures::StreamExt;
use mcl_llm::anthropic::AnthropicBackend;
use mcl_llm::LlmBackend;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn anthropic_success_response(content: &str) -> String {
    format!(r#"{{"content":[{{"type":"text","text":"{content}"}}]}}"#)
}

#[tokio::test]
async fn successful_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(anthropic_success_response("Hello!")),
        )
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "Hello!");
}

#[tokio::test]
async fn empty_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(anthropic_success_response("")))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "");
}

#[tokio::test]
async fn server_error_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 500");
    let msg = format!("{err}");
    assert!(msg.contains("500"), "error should contain 500: {msg}");
}

#[tokio::test]
async fn rate_limit_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_string("rate limited")
                .insert_header("Retry-After", "1"),
        )
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 429");
    let msg = format!("{err}");
    assert!(msg.contains("429"), "error should contain 429: {msg}");
}

#[tokio::test]
async fn malformed_json_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on malformed json");
    let msg = format!("{err}");
    assert!(
        msg.contains("parse error"),
        "error should mention parse error: {msg}"
    );
}

#[tokio::test]
async fn very_long_response() {
    let long_content = "a".repeat(50_000);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(anthropic_success_response(&long_content)),
        )
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let resp = result.expect("should succeed");
    assert_eq!(resp.len(), long_content.len());
}

#[tokio::test]
async fn thinking_tags_stripped() {
    let body = r#"{"content":[{"type":"text","text":"<thinking>I should plan this carefully</thinking>Here is the answer"}]}"#;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "Here is the answer");
}

fn sse_body(events: &[(&str, &str)]) -> String {
    events
        .iter()
        .map(|(event, data)| format!("event: {event}\ndata: {data}\n\n"))
        .collect()
}

#[tokio::test]
async fn anthropic_streaming_yields_delta_text() {
    let sse = sse_body(&[
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#,
        ),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":" world"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ]);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("stream item should be ok"));
    }

    assert_eq!(chunks, vec!["Hello", " world"]);
}

#[tokio::test]
async fn anthropic_streaming_completes_on_message_stop() {
    let sse = sse_body(&[
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"ok"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"unreachable"}}"#,
        ),
    ]);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("stream item should be ok"));
    }

    assert_eq!(chunks, vec!["ok"]);
}

#[tokio::test]
async fn anthropic_streaming_handles_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string(r#"{"error":"unauthorized"}"#))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let item = stream
        .next()
        .await
        .expect("should yield one item before closing");
    let err = item.expect_err("should be an error");
    let msg = format!("{err}");
    assert!(
        msg.contains("401"),
        "error should contain status code: {msg}"
    );
}

#[tokio::test]
async fn anthropic_streaming_ignores_ping_events() {
    let sse = sse_body(&[
        ("ping", r#"{"type":"ping"}"#),
        (
            "content_block_delta",
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"pong"}}"#,
        ),
        ("message_stop", r#"{"type":"message_stop"}"#),
    ]);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("stream item should be ok"));
    }

    assert_eq!(chunks, vec!["pong"]);
}

#[tokio::test]
async fn anthropic_complete_still_works() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(anthropic_success_response("regression check")),
        )
        .mount(&server)
        .await;

    let backend = AnthropicBackend::new("claude-3", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "regression check");
}
