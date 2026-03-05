//! MCP JSON-RPC and HTTP helpers: headers, SSE parsing, tool/resource calls.

pub const MCP_SESSION_ID: &str = "mcp-session-id";
pub const ACCEPT_HEADER: &str = "application/json, text/event-stream";

pub fn parse_sse_first_json(bytes: &[u8]) -> Option<serde_json::Value> {
    for line in std::str::from_utf8(bytes).ok()?.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            return serde_json::from_str(data).ok();
        }
    }
    None
}

pub async fn mcp_read_resource(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    uri: &str,
) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "resources/read",
        "params": { "uri": uri }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id)
        .json(&body)
        .send()
        .await
        .expect("send resources/read");
    let bytes = resp.bytes().await.expect("body");
    let msg = parse_sse_first_json(&bytes).expect("one JSON message");
    if let Some(err) = msg.get("error") {
        let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
        return Err(message);
    }
    Ok(msg)
}

pub async fn mcp_call_tool(
    client: &reqwest::Client,
    base_url: &str,
    session_id: &str,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id)
        .json(&body)
        .send()
        .await
        .expect("send tools/call");
    let bytes = resp.bytes().await.expect("body");
    parse_sse_first_json(&bytes).expect("one JSON message")
}
