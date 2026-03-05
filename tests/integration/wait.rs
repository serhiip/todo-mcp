#[tokio::test]
async fn wait_for_update_returns_after_list_change() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "waittest";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let client2 = reqwest::Client::new();
    let session_id2 = mcp_initialize(&client2, &base_url).await;
    mcp_initialized(&client2, &base_url, &session_id2).await;
    let base_url_waiter = base_url.clone();
    let waiter = tokio::spawn(async move {
        mcp_call_tool(
            &client2,
            &base_url_waiter,
            &session_id2,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "wake", "body": "" }),
    )
    .await;
    let result = waiter.await.expect("waiter task");
    let _ = child.kill();
    assert!(result.get("result").is_some(), "wait-for-update should return result: {:?}", result);
}

#[tokio::test]
async fn wait_for_update_canceled_client_does_not_leak() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "cancelwaiter";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client_updater = reqwest::Client::new();
    let session_updater = mcp_initialize(&client_updater, &base_url).await;
    mcp_initialized(&client_updater, &base_url, &session_updater).await;
    let client_waiter_canceled = reqwest::Client::new();
    let session_waiter_canceled = mcp_initialize(&client_waiter_canceled, &base_url).await;
    mcp_initialized(&client_waiter_canceled, &base_url, &session_waiter_canceled).await;
    let client_waiter2 = reqwest::Client::new();
    let session_waiter2 = mcp_initialize(&client_waiter2, &base_url).await;
    mcp_initialized(&client_waiter2, &base_url, &session_waiter2).await;
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let base_url_c = base_url.clone();
    let canceled_waiter = tokio::spawn(async move {
        tokio::select! {
            _ = cancel_rx => {}
            _ = mcp_call_tool(
                &client_waiter_canceled,
                &base_url_c,
                &session_waiter_canceled,
                "wait-for-update",
                serde_json::json!({ "list_name": list_name }),
            ) => {}
        }
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let _ = cancel_tx.send(());
    let _ = canceled_waiter.await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let base_url_waiter2 = base_url.clone();
    let waiter2 = tokio::spawn(async move {
        mcp_call_tool(
            &client_waiter2,
            &base_url_waiter2,
            &session_waiter2,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let _ = mcp_call_tool(
        &client_updater,
        &base_url,
        &session_updater,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "wake", "body": "" }),
    )
    .await;
    let result = waiter2.await.expect("waiter2 task");
    let _ = child.kill();
    assert!(result.get("result").is_some(), "second waiter should still receive update: {:?}", result);
}
#[tokio::test]
async fn wait_for_update_client_timeout() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "timeoutlist";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400))
        .build()
        .expect("client");
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": { "name": "wait-for-update", "arguments": { "list_name": list_name } }
    });
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        async {
            let resp = client
                .post(format!("{}/mcp", base_url))
                .header("Accept", ACCEPT_HEADER)
                .header("Content-Type", "application/json")
                .header(MCP_SESSION_ID, session_id)
                .json(&body)
                .send()
                .await;
            let resp = resp.expect("send");
            resp.bytes().await
        },
    )
    .await;
    let _ = child.kill();
    assert!(result.is_ok(), "outer timeout should not fire");
}

#[tokio::test]
async fn wait_for_update_concurrent_waiters_both_notified() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "concurrentwaiter";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client_add = reqwest::Client::new();
    let session_add = mcp_initialize(&client_add, &base_url).await;
    mcp_initialized(&client_add, &base_url, &session_add).await;
    let client1 = reqwest::Client::new();
    let session1 = mcp_initialize(&client1, &base_url).await;
    mcp_initialized(&client1, &base_url, &session1).await;
    let client2 = reqwest::Client::new();
    let session2 = mcp_initialize(&client2, &base_url).await;
    mcp_initialized(&client2, &base_url, &session2).await;
    let base_url1 = base_url.clone();
    let base_url2 = base_url.clone();
    let waiter1 = tokio::spawn(async move {
        mcp_call_tool(
            &client1,
            &base_url1,
            &session1,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await
    });
    let waiter2 = tokio::spawn(async move {
        mcp_call_tool(
            &client2,
            &base_url2,
            &session2,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let _ = mcp_call_tool(
        &client_add,
        &base_url,
        &session_add,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "wake", "body": "" }),
    )
    .await;
    let r1 = waiter1.await.expect("waiter1");
    let r2 = waiter2.await.expect("waiter2");
    let _ = child.kill();
    assert!(r1.get("result").is_some(), "waiter1 should get result: {:?}", r1);
    assert!(r2.get("result").is_some(), "waiter2 should get result: {:?}", r2);
}

#[tokio::test]
async fn wait_for_update_multi_waiter_fanout() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "fanout";
    let n_waiters = 15;
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client_add = reqwest::Client::new();
    let session_add = mcp_initialize(&client_add, &base_url).await;
    mcp_initialized(&client_add, &base_url, &session_add).await;
    let mut handles = Vec::new();
    for _ in 0..n_waiters {
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
                "wait-for-update",
                serde_json::json!({ "list_name": list }),
            )
            .await
        }));
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let _ = mcp_call_tool(
        &client_add,
        &base_url,
        &session_add,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "wake", "body": "" }),
    )
    .await;
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await);
    }
    let _ = child.kill();
    let ok_count = results.iter().filter(|r| r.as_ref().map(|v| v.get("result").is_some()).unwrap_or(false)).count();
    assert_eq!(ok_count, n_waiters, "all waiters should unblock: {:?}", results);
}

#[tokio::test]
async fn wait_for_update_no_op_rewrite_does_not_return() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "nooplist";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "one", "body": "" }),
    )
    .await;
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "complete-todo",
        serde_json::json!({ "list_name": list_name, "id": 1 }),
    )
    .await;
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let client2 = reqwest::Client::new();
    let session_id2 = mcp_initialize(&client2, &base_url).await;
    mcp_initialized(&client2, &base_url, &session_id2).await;
    let base_url_waiter = base_url.clone();
    let _waiter = tokio::spawn(async move {
        let res = mcp_call_tool(
            &client2,
            &base_url_waiter,
            &session_id2,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await;
        let _ = tx.send(res);
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "complete-todo",
        serde_json::json!({ "list_name": list_name, "id": 1 }),
    )
    .await;
    tokio::select! {
        _ = tokio::time::sleep(tokio::time::Duration::from_millis(800)) => {}
        msg = &mut rx => {
            let _ = child.kill();
            panic!("wait-for-update should not return after no-op: {:?}", msg);
        }
    }
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "two", "body": "" }),
    )
    .await;
    let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx)
        .await
        .expect("waiter should return within 5s")
        .expect("oneshot");
    let _ = child.kill();
    assert!(result.get("result").is_some(), "{:?}", result);
}

#[tokio::test]
async fn wait_for_update_bounded_time_returns_on_update() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "bounded";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let client2 = reqwest::Client::new();
    let session_id2 = mcp_initialize(&client2, &base_url).await;
    mcp_initialized(&client2, &base_url, &session_id2).await;
    let base_url_waiter = base_url.clone();
    let waiter = tokio::spawn(async move {
        mcp_call_tool(
            &client2,
            &base_url_waiter,
            &session_id2,
            "wait-for-update",
            serde_json::json!({ "list_name": list_name }),
        )
        .await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let _ = mcp_call_tool(
        &client,
        &base_url,
        &session_id,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "x", "body": "" }),
    )
    .await;
    let result = tokio::time::timeout(tokio::time::Duration::from_secs(3), waiter)
        .await
        .expect("wait-for-update must complete within 3s")
        .expect("waiter task");
    let _ = child.kill();
    assert!(result.get("result").is_some(), "{:?}", result);
}
#[tokio::test]
async fn wait_for_update_repeated_cancel_cycles_no_leak() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "leaklist";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    for cycle in 0..8 {
        let client_waiter = reqwest::Client::new();
        let session_waiter = mcp_initialize(&client_waiter, &base_url).await;
        mcp_initialized(&client_waiter, &base_url, &session_waiter).await;
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        let base_url_c = base_url.clone();
        let _dropped = tokio::spawn(async move {
            tokio::select! {
                _ = cancel_rx => {}
                _ = mcp_call_tool(
                    &client_waiter,
                    &base_url_c,
                    &session_waiter,
                    "wait-for-update",
                    serde_json::json!({ "list_name": list_name }),
                ) => {}
            }
        });
        let _ = cancel_tx.send(());
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let client_add = reqwest::Client::new();
        let session_add = mcp_initialize(&client_add, &base_url).await;
        mcp_initialized(&client_add, &base_url, &session_add).await;
        let _ = mcp_call_tool(
            &client_add,
            &base_url,
            &session_add,
            "add-todo",
            serde_json::json!({ "list_name": list_name, "title": format!("cycle {}", cycle), "body": "" }),
        )
        .await;
        let client_waiter2 = reqwest::Client::new();
        let session_waiter2 = mcp_initialize(&client_waiter2, &base_url).await;
        mcp_initialized(&client_waiter2, &base_url, &session_waiter2).await;
        let base_url_w2 = base_url.clone();
        let waiter2 = tokio::spawn(async move {
            mcp_call_tool(
                &client_waiter2,
                &base_url_w2,
                &session_waiter2,
                "wait-for-update",
                serde_json::json!({ "list_name": list_name }),
            )
            .await
        });
        let _ = mcp_call_tool(
            &client_add,
            &base_url,
            &session_add,
            "add-todo",
            serde_json::json!({ "list_name": list_name, "title": format!("cycle {}b", cycle), "body": "" }),
        )
        .await;
        let res = tokio::time::timeout(tokio::time::Duration::from_secs(3), waiter2).await;
        assert!(res.is_ok(), "cycle {}: waiter should still unblock", cycle);
        assert!(res.unwrap().unwrap().get("result").is_some(), "cycle {}", cycle);
    }
    let _ = child.kill();
}

#[tokio::test]
async fn wait_for_update_duplicate_waiter_fanout() {
    let (mut child, _port, dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let list_name = "fanout2";
    std::fs::write(dir.path().join(format!("{}.md", list_name)), "").unwrap();
    let n = 5;
    let mut handles = Vec::new();
    for _ in 0..n {
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
                "wait-for-update",
                serde_json::json!({ "list_name": list }),
            )
            .await
        }));
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    let client_add = reqwest::Client::new();
    let session_add = mcp_initialize(&client_add, &base_url).await;
    mcp_initialized(&client_add, &base_url, &session_add).await;
    let _ = mcp_call_tool(
        &client_add,
        &base_url,
        &session_add,
        "add-todo",
        serde_json::json!({ "list_name": list_name, "title": "x", "body": "" }),
    )
    .await;
    let mut ok = 0;
    for h in handles {
        if let Ok(Ok(res)) = tokio::time::timeout(tokio::time::Duration::from_secs(5), h).await
            && res.get("result").is_some()
        {
            ok += 1;
        }
    }
    let _ = child.kill();
    assert_eq!(ok, n, "each active waiter should receive update exactly once");
}
