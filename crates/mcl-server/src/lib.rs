//! HTTP/WebSocket server for the MCL runtime.
//!
//! Provides network access to MCL sessions for web clients.

pub mod server;

pub use server::{build_router, run_server, AppState, ServerConfig};
