mod support;

use support::*;

#[tokio::test]
async fn initialize_supported_protocol_version_succeeds() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1.0" }
        }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("send");
    let _ = child.kill();
    assert!(resp.status().is_success());
    assert!(resp.headers().get(MCP_SESSION_ID).is_some());
    let bytes = resp.bytes().await.expect("body");
    let msg = parse_sse_first_json(&bytes).expect("json");
    assert!(msg.get("error").is_none(), "supported version should not error: {:?}", msg);
}

#[tokio::test]
async fn initialize_older_protocol_returns_deterministic_response() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2020-01-01",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0.1.0" }
        }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("send");
    let _ = child.kill();
    let bytes = resp.bytes().await.expect("body");
    let msg = parse_sse_first_json(&bytes).expect("json");
    let has_result = msg.get("result").is_some();
    let has_error = msg.get("error").is_some();
    assert!(has_result || has_error, "response must be result or error: {:?}", msg);
    if let Some(err) = msg.get("error") {
        assert!(err.get("code").is_some(), "error must have code: {:?}", err);
        assert!(err.get("message").is_some(), "error must have message: {:?}", err);
    }
}

#[tokio::test]
async fn initialize_malformed_payload_returns_error() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .expect("send");
    let status = resp.status();
    let _ = child.kill();
    let bytes = resp.bytes().await.expect("body");
    let msg = parse_sse_first_json(&bytes);
    assert!(
        status.is_client_error() || msg.as_ref().map(|m| m.get("error").is_some()).unwrap_or(false),
        "malformed initialize should error: status={} msg={:?}",
        status,
        msg
    );
}

#[tokio::test]
async fn error_contract_invalid_list_name_has_code_and_message() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let resp = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "complete-todo",
        serde_json::json!({ "list_name": "!!", "id": 1 }),
    )
    .await;
    let _ = child.kill();
    let err = resp.get("error").expect("error");
    assert!(err.get("code").is_some(), "error must have code: {:?}", err);
    let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
    assert!(!msg.is_empty() && (msg.contains("list_name") || msg.to_lowercase().contains("invalid")), "message: {}", msg);
}

#[tokio::test]
async fn error_contract_missing_id_rejected() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    std::fs::write(dir.path().join("l.md"), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "complete-todo", "arguments": { "list_name": "l" } }
    });
    let resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id)
        .json(&body)
        .send()
        .await
        .expect("send");
    let bytes = resp.bytes().await.expect("body");
    let _ = child.kill();
    let msg = parse_sse_first_json(&bytes).expect("json");
    assert!(msg.get("error").is_some(), "missing id must yield error: {:?}", msg);
}

#[tokio::test]
async fn resource_read_not_found_contract() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    std::fs::write(dir.path().join("valid.md"), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let missing = mcp_read_resource(&client, &base_url, &session_id, "todo://list/nonexistent").await;
    let invalid_uri = mcp_read_resource(&client, &base_url, &session_id, "todo://list/").await;
    let bad_name = mcp_read_resource(&client, &base_url, &session_id, "todo://list/bad!name").await;
    let _ = child.kill();
    let has_error = |r: &Result<serde_json::Value, String>| match r {
        Err(s) => s.contains("not found") || s.contains("resource_not_found"),
        Ok(v) => v.get("error").is_some(),
    };
    assert!(has_error(&missing), "missing list: {:?}", missing);
    assert!(has_error(&invalid_uri), "{:?}", invalid_uri);
    assert!(has_error(&bad_name), "{:?}", bad_name);
}
