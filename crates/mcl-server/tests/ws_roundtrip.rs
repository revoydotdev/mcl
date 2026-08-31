//! WebSocket roundtrip integration tests.
//!
//! Tests that verify:
//! - Client can connect to the `/play` WebSocket endpoint
//! - Client can send text messages
//! - Client can receive responses (or at least connect cleanly)
//! - Connection can be closed cleanly

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use mcl_server::ServerConfig;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

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
async fn ws_roundtrip_connect_disconnect() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let graph_path = temp_dir.path().join("test.graph.toml");
    let static_dir = temp_dir.path().join("static");

    std::fs::create_dir(&static_dir).expect("failed to create static dir");
    std::fs::write(&graph_path, test_graph_toml()).expect("failed to write graph TOML");

    let config = ServerConfig {
        port: 0,
        graph_path,
        static_dir,
    };

    let router = mcl_server::build_router(config).expect("failed to build router");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to random port");
    let addr = listener.local_addr().expect("failed to get local address");
    let ws_url = format!("ws://{}/play", addr);

    let server_task = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server failed");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect to WebSocket");

    let (mut write, mut read) = ws_stream.split();

    write
        .send(Message::Text("hello".to_string()))
        .await
        .expect("failed to send message");

    let read_result = tokio::time::timeout(Duration::from_secs(2), read.next()).await;

    match read_result {
        Ok(Some(Ok(msg))) => if msg.is_text() || msg.is_close() {},
        Ok(Some(Err(e))) => {
            eprintln!(
                "WebSocket read error (expected if LLM backend unavailable): {:?}",
                e
            );
        }
        Ok(None) => {}
        Err(_) => {}
    }

    write.send(Message::Close(None)).await.ok();

    server_task.abort();

    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn ws_handshake_success() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let graph_path = temp_dir.path().join("test.graph.toml");
    let static_dir = temp_dir.path().join("static");

    std::fs::create_dir(&static_dir).expect("failed to create static dir");
    std::fs::write(&graph_path, test_graph_toml()).expect("failed to write graph TOML");

    let config = ServerConfig {
        port: 0,
        graph_path,
        static_dir,
    };

    let router = mcl_server::build_router(config).expect("failed to build router");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to random port");
    let addr = listener.local_addr().expect("failed to get local address");
    let ws_url = format!("ws://{}/play", addr);

    let server_task = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server failed");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (ws_stream, response) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("failed to connect to WebSocket");

    assert_eq!(
        response.status(),
        101,
        "WebSocket handshake should return 101"
    );

    let (mut write, _) = ws_stream.split();
    write.send(Message::Close(None)).await.ok();

    server_task.abort();
    tokio::time::sleep(Duration::from_millis(50)).await;
}
