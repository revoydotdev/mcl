use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use mcl_server::ServerConfig;

fn test_graph_toml() -> String {
    r#"
[graph]
name = "test"

[nodes.client]
type = "client"

[nodes.echo]
type = "llm"

[[edges]]
from = "client"
to = "echo"
channel = "user_input"

[[edges]]
from = "echo"
to = "client"
channel = "display"
"#
    .to_string()
}

#[tokio::test]
async fn get_root_returns_index_html() {
    let temp_dir = tempfile::tempdir().unwrap();
    let graph_path = temp_dir.path().join("test.graph.toml");
    std::fs::write(&graph_path, test_graph_toml()).unwrap();

    let static_dir = temp_dir.path().to_path_buf();
    let index_html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>MCL Test</title>
    <script src="app.js"></script>
</head>
<body>
    <h1>Test Page</h1>
</body>
</html>
"#;
    std::fs::write(static_dir.join("index.html"), index_html).unwrap();

    let config = ServerConfig {
        port: 0,
        graph_path,
        static_dir,
    };
    let router = mcl_server::build_router(config).unwrap();

    let response = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_root_returns_html_content_type() {
    let temp_dir = tempfile::tempdir().unwrap();
    let graph_path = temp_dir.path().join("test.graph.toml");
    std::fs::write(&graph_path, test_graph_toml()).unwrap();

    let static_dir = temp_dir.path().to_path_buf();
    let index_html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>MCL Test</title>
    <script src="app.js"></script>
</head>
<body>
    <h1>Test Page</h1>
</body>
</html>
"#;
    std::fs::write(static_dir.join("index.html"), index_html).unwrap();

    let config = ServerConfig {
        port: 0,
        graph_path,
        static_dir,
    };
    let router = mcl_server::build_router(config).unwrap();

    let response = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type header missing");
    let content_type_str = content_type.to_str().unwrap();
    assert!(content_type_str.contains("text/html"));
}

#[tokio::test]
async fn get_root_returns_html_with_expected_elements() {
    let temp_dir = tempfile::tempdir().unwrap();
    let graph_path = temp_dir.path().join("test.graph.toml");
    std::fs::write(&graph_path, test_graph_toml()).unwrap();

    let static_dir = temp_dir.path().to_path_buf();
    let index_html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>MCL Test</title>
    <script src="app.js"></script>
</head>
<body>
    <h1>Test Page</h1>
</body>
</html>
"#;
    std::fs::write(static_dir.join("index.html"), index_html).unwrap();

    let config = ServerConfig {
        port: 0,
        graph_path,
        static_dir,
    };
    let router = mcl_server::build_router(config).unwrap();

    let response = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert!(body_str.contains("<title>MCL Test</title>"));
    assert!(body_str.contains("<script src=\"app.js\"></script>"));
    assert!(body_str.contains("<h1>Test Page</h1>"));
}
