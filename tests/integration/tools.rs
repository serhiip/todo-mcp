#[tokio::test]
async fn invalid_tool_arguments_return_error() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    std::fs::write(dir.path().join("ok.md"), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let empty_list = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": "", "title": "x", "body": "" }),
    )
    .await;
    let _ = child.kill();
    let err = empty_list.get("error").expect("expected error response");
    assert!(err.get("message").and_then(|m| m.as_str()).unwrap_or("").contains("list_name") || err.get("code").is_some());
}

#[tokio::test]
async fn invalid_list_name_tool_returns_error() {
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
        serde_json::json!({ "list_name": "bad name!", "id": 1 }),
    )
    .await;
    let _ = child.kill();
    assert!(resp.get("error").is_some(), "expected error: {:?}", resp);
}

#[tokio::test]
async fn add_todo_body_length_boundary() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "bodylen";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let at_limit = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "t", "body": "x".repeat(4096) }),
    )
    .await;
    let over_limit = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "t", "body": "x".repeat(4097) }),
    )
    .await;
    let _ = child.kill();
    assert!(at_limit.get("result").is_some(), "4096 body should succeed: {:?}", at_limit);
    assert!(over_limit.get("error").is_some(), "4097 body should be rejected: {:?}", over_limit);
}

#[tokio::test]
async fn complete_todo_idempotency_under_concurrent_calls() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "idem";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client_add = reqwest::Client::new();
    let session_add = mcp_initialize(&client_add, &base_url).await;
    mcp_initialized(&client_add, &base_url, &session_add).await;
    let _ = mcp_call_tool(
        &client_add,
        &base_url,
        &session_add,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "one", "body": "" }),
    )
    .await;
    let mut handles = Vec::new();
    for _ in 0..20 {
        let client = reqwest::Client::new();
        let session_id = mcp_initialize(&client, &base_url).await;
        mcp_initialized(&client, &base_url, &session_id).await;
        let base_url_c = base_url.clone();
        let list = list_name.to_string();
        handles.push(tokio::spawn(async move {
            mcp_call_tool(
                &client,
                &base_url_c,
                &session_id,
                "complete-todo",
                serde_json::json!({ "list_name": list, "id": 1 }),
            )
            .await
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    let client_read = reqwest::Client::new();
    let session_read = mcp_initialize(&client_read, &base_url).await;
    mcp_initialized(&client_read, &base_url, &session_read).await;
    let resource = mcp_read_resource(&client_read, &base_url, &session_read, &format!("todo://list/{}", list_name)).await;
    let _ = child.kill();
    let content: String = resource
        .ok()
        .and_then(|r| r.get("result").and_then(|x| x.get("contents")).and_then(|c| c.as_array()).and_then(|a| a.first()).and_then(|c| c.get("text")).and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_default();
    assert!(content.contains("[x]"), "item should be completed: {}", content);
}

#[tokio::test]
async fn add_todo_title_body_limits() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "limits";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let empty_title = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "", "body": "" }),
    )
    .await;
    let _ = child.kill();
    assert!(empty_title.get("error").is_some(), "empty title should be rejected: {:?}", empty_title);
    let (mut child2, _port2, dir2, base_url2) = spawn_server();
    if !wait_health(&base_url2).await {
        let _ = child2.kill();
        panic!("server2 not ready");
    }
    std::fs::write(dir2.path().join("l.md"), "").unwrap();
    let session_id2 = mcp_initialize(&client, &base_url2).await;
    mcp_initialized(&client, &base_url2, &session_id2).await;
    let near_limit = mcp_call_tool(
        &client,
        &base_url2,
        &session_id2,
        "add-todo",
        serde_json::json!({ "list_name": "l", "title": "a".repeat(200), "body": "b".repeat(100) }),
    )
    .await;
    let _ = child2.kill();
    assert!(near_limit.get("result").is_some(), "near-limit should succeed: {:?}", near_limit);
}

