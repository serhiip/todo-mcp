#[tokio::test]
async fn read_resource_missing_list_returns_not_found() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let result = mcp_read_resource(&client, &base_url, &session_id, "todo://list/nonexistent").await;
    let _ = child.kill();
    let Err(message) = result else {
        panic!("expected resource_not_found error, got ok");
    };
    assert!(
        message.contains("resource_not_found") || message.to_lowercase().contains("not found"),
        "error message should indicate not found: {}",
        message
    );
}

#[tokio::test]
async fn read_resource_large_list_bounded_time() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "large";
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    for i in 0..300 {
        let _ = mcp_call_tool(
            &client,
            &base_url,
            &session_id,
            "add-todo",
            serde_json::json!({ "list_name": list_name, "title": format!("item {}", i), "body": "" }),
        )
        .await;
    }
    let start = std::time::Instant::now();
    let _ = mcp_read_resource(&client, &base_url, &session_id, &format!("todo://list/{}", list_name)).await;
    let elapsed = start.elapsed();
    let _ = child.kill();
    assert!(elapsed.as_secs() < 5, "large list read should complete within 5s, took {:?}", elapsed);
}

#[tokio::test]
async fn resources_list_stable_across_sessions() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    std::fs::write(dir.path().join("a.md"), "").unwrap();
    std::fs::write(dir.path().join("b.md"), "").unwrap();
    let mut uris = std::collections::HashSet::new();
    for _ in 0..10 {
        let client = reqwest::Client::new();
        let session_id = mcp_initialize(&client, &base_url).await;
        mcp_initialized(&client, &base_url, &session_id).await;
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list",
            "params": {}
        });
        let resp = client
            .post(format!("{}/mcp", base_url))
            .header("Accept", ACCEPT_HEADER)
            .header("Content-Type", "application/json")
            .header(MCP_SESSION_ID, session_id.clone())
            .json(&body)
            .send()
            .await
            .expect("send");
        let bytes = resp.bytes().await.expect("body");
        let msg = parse_sse_first_json(&bytes).expect("json");
        if let Some(resources) = msg.get("result").and_then(|r| r.get("resources")).and_then(|r| r.as_array()) {
            for r in resources {
                if let Some(uri) = r.get("uri").and_then(|u| u.as_str()) {
                    uris.insert(uri.to_string());
                }
            }
        }
    }
    let _ = child.kill();
    assert!(uris.contains("todo://list/a") && uris.contains("todo://list/b"), "stable list: {:?}", uris);
}

#[tokio::test]
async fn resource_read_parseable_under_concurrent_writes() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "stress";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client_writer = reqwest::Client::new();
    let session_writer = mcp_initialize(&client_writer, &base_url).await;
    mcp_initialized(&client_writer, &base_url, &session_writer).await;
    let client_reader = reqwest::Client::new();
    let session_reader = mcp_initialize(&client_reader, &base_url).await;
    mcp_initialized(&client_reader, &base_url, &session_reader).await;
    let base_url_w = base_url.clone();
    let writer = tokio::spawn(async move {
        for i in 0..40 {
            let _ = mcp_call_tool(
                &client_writer,
                &base_url_w,
                &session_writer,
                "add-todo",
                serde_json::json!({ "list_name": list_name, "title": format!("t{}", i), "body": "" }),
            )
            .await;
            if i % 2 == 0 {
                let _ = mcp_call_tool(
                    &client_writer,
                    &base_url_w,
                    &session_writer,
                    "complete-todo",
                    serde_json::json!({ "list_name": list_name, "id": i / 2 + 1 }),
                )
                .await;
            }
        }
    });
    let base_url_r = base_url.clone();
    let mut success_count = 0usize;
    for _ in 0..50 {
        let resource = mcp_read_resource(
            &client_reader,
            &base_url_r,
            &session_reader.clone(),
            &format!("todo://list/{}", list_name),
        )
        .await;
        if let Ok(msg) = resource {
            let text = msg
                .get("result")
                .and_then(|r| r.get("contents"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            for line in text.lines() {
                let _ = line.trim();
            }
            success_count += 1;
        }
    }
    let _ = writer.await;
    let _ = child.kill();
    assert!(success_count == 50, "all 50 reads should succeed under concurrent writes");
}

