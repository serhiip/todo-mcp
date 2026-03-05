#[tokio::test]
async fn parallel_add_complete_preserves_consistency() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "racelist";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let n_clients = 8;
    let ops_per_client = 20;
    let mut handles = Vec::new();
    for _ in 0..n_clients {
        let base_url = base_url.clone();
        let list = list_name.to_string();
        let h = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let session_id = mcp_initialize(&client, &base_url).await;
            mcp_initialized(&client, &base_url, &session_id).await;
            let mut ids = Vec::new();
            for i in 0..ops_per_client {
                let resp = mcp_call_tool(
                    &client,
                    &base_url,
                    &session_id,
                    "add-todo",
                    serde_json::json!({
                        "list_name": list,
                        "title": format!("item-{:?}-{}", std::thread::current().id(), i),
                        "body": ""
                    }),
                )
                .await;
                if let Some(result) = resp.get("result")
                    && let Some(content) = result.get("content").and_then(|c| c.as_array()).and_then(|a| a.first())
                    && let Some(text) = content.get("text").and_then(|t| t.as_str())
                    && let Some(hash) = text.find('#')
                {
                    let after = &text[hash + 1..];
                    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() && let Ok(id) = digits.parse::<u32>() {
                        ids.push(id);
                    }
                }
            }
            for id in ids {
                let _ = mcp_call_tool(
                    &client,
                    &base_url,
                    &session_id,
                    "complete-todo",
                    serde_json::json!({ "list_name": list, "id": id }),
                )
                .await;
            }
        });
        handles.push(h);
    }
    for h in handles {
        h.await.expect("client task");
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let resource = mcp_read_resource(&client, &base_url, &session_id, &format!("todo://list/{}", list_name))
        .await
        .expect("read resource");
    let _ = child.kill();
    let content = resource
        .get("result")
        .and_then(|r| r.get("contents"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    let lines: Vec<&str> = content.lines().filter(|l| l.trim().starts_with("- [")).collect();
    let completed_count = lines.iter().filter(|l| l.contains("[x]")).count();
    let total = lines.len();
    assert_eq!(total, n_clients * ops_per_client, "no lost adds; total items");
    assert_eq!(completed_count, total, "all items completed; no lost updates");
}

#[tokio::test]
async fn high_load_add_complete_latency_guard() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "load";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let start = std::time::Instant::now();
    let n_ops = 30;
    for i in 0..n_ops {
        let _ = mcp_call_tool(
            &client,
            &base_url,
            &session_id,
            "add-todo",
            serde_json::json!({ "list_name": list_name, "title": format!("item {}", i), "body": "" }),
        )
        .await;
    }
    for id in 1..=n_ops {
        let _ = mcp_call_tool(
            &client,
            &base_url,
            &session_id,
            "complete-todo",
            serde_json::json!({ "list_name": list_name, "id": id }),
        )
        .await;
    }
    let elapsed = start.elapsed();
    let _ = child.kill();
    assert!(elapsed.as_secs() < 20, "high-load ops should complete within 20s, took {:?}", elapsed);
}

#[tokio::test]
async fn cross_list_isolation_under_concurrency() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    for name in ["listA", "listB", "listC"] {
        std::fs::write(dir.path().join(format!("{}.md", name)), "").unwrap();
    }
    let mut handles = Vec::new();
    for (i, list) in ["listA", "listB", "listC"].iter().enumerate() {
        let client = reqwest::Client::new();
        let session_id = mcp_initialize(&client, &base_url).await;
        mcp_initialized(&client, &base_url, &session_id).await;
        let base_url_c = base_url.clone();
        let list = (*list).to_string();
        handles.push(tokio::spawn(async move {
            for j in 0..5 {
                let _ = mcp_call_tool(
                    &client,
                    &base_url_c,
                    &session_id,
                    "add-todo",
                    serde_json::json!({ "list_name": list, "title": format!("{}-{}", i, j), "body": "" }),
                )
                .await;
            }
            let _ = mcp_call_tool(
                &client,
                &base_url_c,
                &session_id,
                "complete-todo",
                serde_json::json!({ "list_name": list, "id": 1 }),
            )
            .await;
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    for list in ["listA", "listB", "listC"] {
        let r = mcp_read_resource(&client, &base_url, &session_id.clone(), &format!("todo://list/{}", list)).await;
        let content: String = r
            .ok()
            .and_then(|v| v.get("result").and_then(|x| x.get("contents")).and_then(|c| c.as_array()).and_then(|a| a.first()).and_then(|c| c.get("text")).and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default();
        let completed: Vec<_> = content.lines().filter(|l| l.contains("[x]")).collect();
        let pending: Vec<_> = content.lines().filter(|l| l.contains("[ ]")).collect();
        assert_eq!(completed.len(), 1, "{} should have exactly one completed", list);
        assert_eq!(pending.len(), 4, "{} should have 4 pending", list);
    }
    let _ = child.kill();
}

#[tokio::test]
async fn list_names_stable_under_concurrent_activity() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    for name in ["a", "b", "c"] {
        std::fs::write(dir.path().join(format!("{}.md", name)), "").unwrap();
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let base_url_c = base_url.clone();
    let add_task = tokio::spawn(async move {
        for i in 0..20 {
            let list = ["a", "b", "c"][i % 3];
            let _ = mcp_call_tool(
                &client,
                &base_url_c,
                &session_id,
                "add-todo",
                serde_json::json!({ "list_name": list, "title": format!("x{}", i), "body": "" }),
            )
            .await;
        }
    });
    let client2 = reqwest::Client::new();
    let session_id2 = mcp_initialize(&client2, &base_url).await;
    mcp_initialized(&client2, &base_url, &session_id2).await;
    let mut names_seen = std::collections::HashSet::new();
    for _ in 0..30 {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list",
            "params": {}
        });
        let resp = client2
            .post(format!("{}/mcp", base_url))
            .header("Accept", ACCEPT_HEADER)
            .header("Content-Type", "application/json")
            .header(MCP_SESSION_ID, session_id2.clone())
            .json(&body)
            .send()
            .await
            .expect("send");
        let bytes = resp.bytes().await.expect("body");
        let msg = parse_sse_first_json(&bytes).expect("json");
        if let Some(result) = msg.get("result").and_then(|r| r.get("resources")).and_then(|r| r.as_array()) {
            for r in result {
                if let Some(uri) = r.get("uri").and_then(|u| u.as_str()) {
                    let name = uri.strip_prefix("todo://list/").unwrap_or(uri);
                    assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "sanitized name: {}", name);
                    names_seen.insert(name.to_string());
                }
            }
        }
    }
    let _ = add_task.await;
    let _ = child.kill();
    assert!(names_seen.contains("a") && names_seen.contains("b") && names_seen.contains("c"));
}

