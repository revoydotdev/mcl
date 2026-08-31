use futures::StreamExt;
use mcl_llm::openai::OpenAiBackend;
use mcl_llm::LlmBackend;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn openai_success_response(content: &str) -> String {
    format!(r#"{{"choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}]}}"#)
}

fn sse_response(chunks: &[&str]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{chunk}\"}}}}]}}\n\n"
        ));
    }
    body.push_str("data: [DONE]\n\n");
    body
}

#[tokio::test]
async fn successful_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(openai_success_response("Hello!")))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "Hello!");
}

#[tokio::test]
async fn empty_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(openai_success_response("")))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("should succeed"), "");
}

#[tokio::test]
async fn server_error_500() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 500");
    let msg = format!("{err}");
    assert!(msg.contains("500"), "error should contain 500: {msg}");
}

#[tokio::test]
async fn rate_limit_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_string("rate limited")
                .insert_header("Retry-After", "2"),
        )
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on 429");
    let msg = format!("{err}");
    assert!(msg.contains("429"), "error should contain 429: {msg}");
}

#[tokio::test]
async fn malformed_json_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{{invalid json"))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail on malformed json");
    let msg = format!("{err}");
    assert!(
        msg.contains("parse error"),
        "error should mention parse error: {msg}"
    );
}

#[tokio::test]
async fn no_choices_in_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"choices":[]}"#))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let err = result.expect_err("should fail with no choices");
    let msg = format!("{err}");
    assert!(
        msg.contains("no choices"),
        "error should mention no choices: {msg}"
    );
}

#[tokio::test]
async fn very_long_response() {
    let long_content = "a".repeat(50_000);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(openai_success_response(&long_content)),
        )
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    let resp = result.expect("should succeed");
    assert_eq!(resp.len(), long_content.len());
}

#[tokio::test]
async fn openai_streaming_yields_delta_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(sse_response(&["Hello", " world", "!"])),
        )
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("chunk should be Ok"));
    }

    assert_eq!(chunks, vec!["Hello", " world", "!"]);
}

#[tokio::test]
async fn openai_streaming_completes_on_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse_response(&["done"])))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        count += 1;
        assert!(item.is_ok(), "all chunks should be Ok: {item:?}");
    }
    assert_eq!(count, 1, "should yield exactly one chunk then terminate");
}

#[tokio::test]
async fn openai_streaming_handles_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
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
async fn openai_streaming_handles_malformed_sse() {
    let server = MockServer::start().await;
    let body = "data: {not valid json}\n\ndata: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let result = stream.next().await.expect("should produce one item");
    let err = result.expect_err("should be an error on malformed json");
    let msg = format!("{err}");
    assert!(msg.contains("parse"), "error should mention parse: {msg}");
}

#[tokio::test]
async fn openai_streaming_skips_empty_deltas() {
    let server = MockServer::start().await;
    let body = [
        r#"data: {"choices":[{"delta":{"content":""}}]}"#,
        r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#,
        "data: [DONE]",
    ]
    .join("\n\n");
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let mut stream = backend.stream("", &[], "hi");

    let mut chunks = Vec::new();
    while let Some(item) = stream.next().await {
        chunks.push(item.expect("chunk should be Ok"));
    }

    assert_eq!(chunks, vec!["hi"]);
}

#[tokio::test]
async fn openai_complete_still_works() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(openai_success_response("Still works!")),
        )
        .mount(&server)
        .await;

    let backend = OpenAiBackend::new("gpt-4", "test-key").with_url(server.uri());
    let result = backend.complete("", &[], "hi").await;
    assert_eq!(result.expect("complete should succeed"), "Still works!");
}
