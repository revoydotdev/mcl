use futures::StreamExt;
use mcl_llm::ollama::OllamaBackend;
use mcl_llm::LlmBackend;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn ollama_success_response(content: &str) -> String {
    format!(r#"{{"message":{{"role":"assistant","content":"{content}"}}}}"#)
}

fn ollama_error_response(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_string(body)
}

fn ollama_ndjson_stream_response(chunks: &[&str]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!(
            r#"{{"message":{{"content":"{}"}},"done":false}}"#,
            chunk
        ));
        body.push('\n');
    }
    body.push_str(r#"{"done":true}"#);
    body.push('\n');
    body
}

#[tokio::test]
async fn successful_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ollama_success_response("Hello!")))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "Hello!");
}

#[tokio::test]
async fn empty_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ollama_success_response("")))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "");
}

#[tokio::test]
async fn server_error_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ollama_error_response(500, "internal server error"))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 500");
    let msg = format!("{err}");
    assert!(msg.contains("500"), "error should contain 500: {msg}");
}

#[tokio::test]
async fn rate_limit_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ollama_error_response(429, "rate limited"))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 429");
    let msg = format!("{err}");
    assert!(msg.contains("429"), "error should contain 429: {msg}");
}

#[tokio::test]
async fn malformed_json_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json at all"))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
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
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(ollama_success_response(&long_content)),
        )
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let resp = result.expect("should succeed");
    assert_eq!(resp.len(), long_content.len());
}

#[tokio::test]
async fn whitespace_trimmed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(ollama_success_response("  padded  ")),
        )
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "padded");
}

#[tokio::test]
async fn ollama_streaming_yields_chunks() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ollama_ndjson_stream_response(&["Hello", " world", "!"])),
        )
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("chunk should be Ok"));
    }

    assert_eq!(chunks, vec!["Hello", " world", "!"]);
}

#[tokio::test]
async fn ollama_streaming_completes_on_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(ollama_ndjson_stream_response(&["done"])),
        )
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        count += 1;
        assert!(item.is_ok(), "all chunks should be Ok: {item:?}");
    }
    assert_eq!(count, 1, "should yield exactly one chunk then terminate");
}

#[tokio::test]
async fn ollama_streaming_handles_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let result = stream.next().await.expect("should produce one item");
    let err = result.expect_err("should be an error");
    let msg = format!("{err}");
    assert!(
        msg.contains("500"),
        "error should contain status code: {msg}"
    );
}

#[tokio::test]
async fn ollama_complete_still_works() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(ollama_success_response("Still works!")),
        )
        .mount(&server)
        .await;

    let backend = OllamaBackend::new("llama3").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("complete should succeed"), "Still works!");
}
