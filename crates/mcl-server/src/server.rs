use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde::Serialize;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use mcl_graph::{GraphDefParser, GraphError, NodeFactory, TomlGraphDefParser};
use mcl_nodes::ws_client::WsBridgeNode;

#[derive(Clone)]
pub struct AppState {
    pub graph_def: Arc<mcl_graph::GraphDef>,
    pub static_dir: PathBuf,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
}

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub graph_path: PathBuf,
    pub static_dir: PathBuf,
}

pub fn build_router(config: ServerConfig) -> Result<Router, GraphError> {
    let parser = TomlGraphDefParser;
    let graph_def = parser.parse_file(&config.graph_path)?;

    info!(
        "loaded graph: {} ({} nodes, {} edges)",
        graph_def.name,
        graph_def.nodes.len(),
        graph_def.edges.len()
    );

    let state = AppState {
        graph_def: Arc::new(graph_def),
        static_dir: config.static_dir,
    };

    let router = Router::new()
        .route("/play", get(ws_play_handler))
        .route("/health", get(health_handler))
        .nest_service("/", ServeDir::new(&state.static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state);

    Ok(router)
}

pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let router = build_router(config.clone())?;
    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("MCL server listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

async fn health_handler() -> impl IntoResponse {
    axum::Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn ws_play_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_session(socket, state))
}

async fn handle_ws_session(socket: WebSocket, state: AppState) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel::<String>();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();

    let ws_bridge = WsBridgeNode::new(inbound_rx, outbound_tx);

    let mut factory = NodeFactory::new();
    mcl_nodes::register_default_nodes(&mut factory);

    let graph_def = (*state.graph_def).clone();

    let mut executor = match mcl_graph::GraphExecutor::build_with_override(
        &graph_def,
        &factory,
        "ws_client",
        Box::new(ws_bridge),
    )
    .await
    {
        Ok(exec) => exec,
        Err(e) => {
            error!("failed to build session executor: {e}");
            let _ = ws_sink.send(Message::Text(format!("[error: {e}]"))).await;
            return;
        }
    };

    if let Err(e) = executor.run_until_quiescent().await {
        error!("initial graph execution failed: {e}");
    }

    let session_loop = async {
        loop {
            tokio::select! {
                result = executor.poll_sources() => {
                    match result {
                        Ok(true) => {
                            if let Err(e) = executor.run_until_quiescent().await {
                                error!("graph step error: {e}");
                                break;
                            }
                        }
                        Ok(false) => {}
                        Err(e) => {
                            if e.to_string() == "execution error: user quit" {
                                info!("user quit session");
                            } else {
                                error!("source poll error: {e}");
                            }
                            break;
                        }
                    }
                }
                msg = outbound_rx.recv() => {
                    match msg {
                        Some(text) => {
                            if ws_sink.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    };

    let ws_read = async {
        while let Some(Ok(msg)) = ws_stream.next().await {
            if let Message::Text(text) = msg {
                if inbound_tx.send(text.to_string()).is_err() {
                    break;
                }
            } else if let Message::Close(_) = msg {
                break;
            }
        }
        drop(inbound_tx);
    };

    tokio::select! {
        _ = session_loop => {}
        _ = ws_read => {}
    }

    info!("WebSocket session ended");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

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
    async fn health_endpoint_returns_ok() {
        let temp_dir = tempfile::tempdir().unwrap();
        let graph_path = temp_dir.path().join("test.graph.toml");
        std::fs::write(&graph_path, test_graph_toml()).unwrap();
        let static_dir = temp_dir.path().to_path_buf();

        let config = ServerConfig {
            port: 0,
            graph_path,
            static_dir,
        };
        let router = build_router(config).unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_endpoint_returns_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let graph_path = temp_dir.path().join("test.graph.toml");
        std::fs::write(&graph_path, test_graph_toml()).unwrap();
        let static_dir = temp_dir.path().to_path_buf();

        let config = ServerConfig {
            port: 0,
            graph_path,
            static_dir,
        };
        let router = build_router(config).unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[test]
    fn build_router_with_invalid_graph_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let graph_path = temp_dir.path().join("bad.graph.toml");
        std::fs::write(&graph_path, "invalid toml").unwrap();
        let static_dir = temp_dir.path().to_path_buf();

        let config = ServerConfig {
            port: 0,
            graph_path,
            static_dir,
        };
        let result = build_router(config);
        assert!(result.is_err());
    }
}
