//! MCP session lifecycle: initialize and initialized notification.

use super::rpc::{ACCEPT_HEADER, MCP_SESSION_ID};

pub async fn mcp_initialize(client: &reqwest::Client, base_url: &str) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "integration-test", "version": "0.1.0" }
        }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("send initialize");
    let session_id = resp
        .headers()
        .get(MCP_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .expect("Mcp-Session-Id header")
        .to_string();
    let _ = resp.bytes().await;
    session_id
}

pub async fn mcp_initialized(client: &reqwest::Client, base_url: &str, session_id: &str) {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id)
        .json(&body)
        .send()
        .await
        .expect("send initialized");
    assert!(resp.status().as_u16() == 202 || resp.status().is_success());
}
