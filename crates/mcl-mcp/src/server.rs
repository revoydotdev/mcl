use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

use crate::tools::McpToolHandler;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    id: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    result: serde_json::Value,
    id: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcError {
    jsonrpc: String,
    error: JsonRpcErrorBody,
    id: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcErrorBody {
    code: i32,
    message: String,
}

pub struct McpServer {
    handler: McpToolHandler,
}

impl McpServer {
    pub fn new(handler: McpToolHandler) -> Self {
        Self { handler }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => self.handle_request(req),
                Err(e) => match serde_json::to_string(&JsonRpcError {
                    jsonrpc: "2.0".into(),
                    error: JsonRpcErrorBody {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    },
                    id: serde_json::Value::Null,
                }) {
                    Ok(s) => s,
                    Err(_) => r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"internal serialize error"},"id":null}"#.into(),
                },
            };

            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }

        Ok(())
    }

    fn handle_request(&mut self, req: JsonRpcRequest) -> String {
        let result = match req.method.as_str() {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(&req.params),
            "resources/list" => self.handle_resources_list(),
            "resources/read" => self.handle_resources_read(&req.params),
            _ => Err((-32601, format!("Method not found: {}", req.method))),
        };

        match result {
            Ok(value) => match serde_json::to_string(&JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: value,
                id: req.id,
            }) {
                Ok(s) => s,
                Err(_) => r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"internal serialize error"},"id":null}"#.into(),
            },
            Err((code, message)) => match serde_json::to_string(&JsonRpcError {
                jsonrpc: "2.0".into(),
                error: JsonRpcErrorBody { code, message },
                id: req.id,
            }) {
                Ok(s) => s,
                Err(_) => r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"internal serialize error"},"id":null}"#.into(),
            },
        }
    }

    fn handle_initialize(&self) -> Result<serde_json::Value, (i32, String)> {
        Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "resources": {}
            },
            "serverInfo": {
                "name": "mcl-mcp",
                "version": "0.1.0"
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<serde_json::Value, (i32, String)> {
        Ok(serde_json::json!({
            "tools": self.handler.list_tools()
        }))
    }

    fn handle_tools_call(
        &mut self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let name = params["name"]
            .as_str()
            .ok_or((-32602, "Missing tool name".into()))?;
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        self.handler.call_tool(name, &args)
    }

    fn handle_resources_list(&self) -> Result<serde_json::Value, (i32, String)> {
        Ok(serde_json::json!({
            "resources": self.handler.list_resources()
        }))
    }

    fn handle_resources_read(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, (i32, String)> {
        let uri = params["uri"]
            .as_str()
            .ok_or((-32602, "Missing resource URI".into()))?;
        self.handler.read_resource(uri)
    }
}
