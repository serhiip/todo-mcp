// CLI integration: add subcommand calls server; verify todo creation, resource content, and wait-for-update wakeup.

#[tokio::test]
async fn cli_add_creates_todo_and_appears_in_resource() {
    let (mut child, port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let bin = std::env::var("CARGO_BIN_EXE_todo-mcp").unwrap_or_else(|_| "target/debug/todo-mcp".to_string());
    let out = std::process::Command::new(bin)
        .env("MCP_PORT", port.to_string())
        .args(["add", "cli_list", "CLI Title", "CLI body line"])
        .output()
        .expect("run cli add");
    assert!(out.status.success(), "cli add must succeed: stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let id: u32 = stdout.trim().parse().expect("stdout is new id");
    assert_eq!(id, 1);

    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let resource = mcp_read_resource(&client, &base_url, &session_id, "todo://list/cli_list")
        .await
        .expect("read resource");
    let text = resource
        .get("result")
        .and_then(|r| r.get("contents"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(text.contains("CLI Title"), "resource should contain title: {}", text);
    assert!(text.contains("CLI body line"), "resource should contain body: {}", text);
    assert!(text.contains("#1") || text.contains(&format!("#{}", id)), "resource should contain id: {}", text);

    let _ = child.kill();
}

#[tokio::test]
async fn cli_add_wakeup_wait_for_update() {
    let (mut child, port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let client2 = client.clone();
    let base_url2 = base_url.clone();
    let session_id2 = session_id.clone();
    let wait_handle = tokio::spawn(async move {
        let _ = tokio::time::timeout(
            tokio::time::Duration::from_secs(8),
            mcp_call_tool(
                &client2,
                &base_url2,
                &session_id2,
                "wait-for-update",
                serde_json::json!({ "list_name": "wakeup_list" }),
            ),
        )
        .await;
        let _ = tx.send(());
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let bin = std::env::var("CARGO_BIN_EXE_todo-mcp").unwrap_or_else(|_| "target/debug/todo-mcp".to_string());
    let out = std::process::Command::new(bin)
        .env("MCP_PORT", port.to_string())
        .args(["add", "wakeup_list", "Wake", ""])
        .output()
        .expect("run cli add");
    assert!(out.status.success(), "cli add must succeed: {:?}", out.stderr);

    let res = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx).await;
    assert!(res.is_ok(), "wait-for-update should return after CLI add");
    let _ = wait_handle.await;
    let _ = child.kill();
}

#[tokio::test]
async fn cli_add_json_output_returns_structured_result() {
    let (mut child, port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let bin = cli_bin();
    let out = std::process::Command::new(&bin)
        .env("MCP_PORT", port.to_string())
        .args(["add", "--json", "json_list", "JSON Title", "body"])
        .output()
        .expect("run cli add --json");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let obj: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(obj.get("id").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(obj.get("list").and_then(|v| v.as_str()), Some("json_list"));
    assert_eq!(obj.get("title").and_then(|v| v.as_str()), Some("JSON Title"));
    assert!(obj.get("server").and_then(|v| v.as_str()).unwrap_or("").contains("127.0.0.1"));
    let _ = child.kill();
}

#[tokio::test]
async fn cli_add_with_explicit_port_reaches_server() {
    let (mut child, port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let bin = std::env::var("CARGO_BIN_EXE_todo-mcp").unwrap_or_else(|_| "target/debug/todo-mcp".to_string());
    let out = std::process::Command::new(bin)
        .args(["add", "--port", &port.to_string(), "port_list", "From --port", ""])
        .output()
        .expect("run cli add");
    assert!(out.status.success(), "cli add with --port must succeed: stderr={}", String::from_utf8_lossy(&out.stderr));
    let id: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("stdout is id");
    assert_eq!(id, 1);
    let client = reqwest::Client::new();
    let session_id = mcp_initialize(&client, &base_url).await;
    mcp_initialized(&client, &base_url, &session_id).await;
    let resource = mcp_read_resource(&client, &base_url, &session_id, "todo://list/port_list").await.expect("read");
    let text = resource
        .get("result")
        .and_then(|r| r.get("contents"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(text.contains("From --port"), "resource should contain title: {}", text);
    let _ = child.kill();
}

fn cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_todo-mcp").unwrap_or_else(|_| "target/debug/todo-mcp".to_string())
}

#[test]
fn cli_add_no_args_exits_input_and_prints_usage() {
    let out = std::process::Command::new(cli_bin())
        .args(["add"])
        .output()
        .expect("run cli add");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage") || stderr.contains("Error"), "stderr should mention usage: {}", stderr);
}

#[test]
fn cli_add_one_positional_exits_input() {
    let out = std::process::Command::new(cli_bin())
        .args(["add", "onlylist"])
        .output()
        .expect("run cli add");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("usage") || stderr.contains("title"), "stderr: {}", stderr);
}

#[test]
fn cli_add_server_missing_value_exits_input() {
    let out = std::process::Command::new(cli_bin())
        .args(["add", "--server"])
        .output()
        .expect("run cli add");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("server") || stderr.contains("URL") || stderr.contains("usage"), "stderr: {}", stderr);
}

#[test]
fn cli_add_port_invalid_exits_input() {
    let out = std::process::Command::new(cli_bin())
        .args(["add", "--port", "notanum", "mylist", "Title", ""])
        .output()
        .expect("run cli add");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("port") || stderr.contains("invalid") || stderr.contains("usage"), "stderr: {}", stderr);
}

#[test]
fn cli_help_exits_zero_and_prints_usage() {
    let out = std::process::Command::new(cli_bin())
        .args(["--help"])
        .output()
        .expect("run cli --help");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{} {}", stdout, stderr);
    assert!(combined.contains("add") && (combined.contains("usage") || combined.contains("USAGE")), "output: {}", combined);
}

#[test]
fn cli_add_help_exits_zero() {
    let out = std::process::Command::new(cli_bin())
        .args(["add", "--help"])
        .output()
        .expect("run cli add --help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{} {}", stdout, stderr);
    assert!(combined.contains("add") && (combined.contains("usage") || combined.contains("USAGE")), "output: {}", combined);
}

#[test]
fn cli_add_empty_title_exits_input_with_hint() {
    let out = std::process::Command::new(cli_bin())
        .args(["add", "mylist", ""])
        .output()
        .expect("run cli add");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("title") || stderr.contains("required"),
        "stderr should hint about title: {}",
        stderr
    );
}

#[tokio::test]
async fn cli_add_summary_prints_two_lines_second_is_id() {
    let (mut child, port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let out = std::process::Command::new(cli_bin())
        .env("MCP_PORT", port.to_string())
        .args(["add", "--summary", "sum_list", "Summary Title", ""])
        .output()
        .expect("run cli add --summary");
    let _ = child.kill();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 2, "expected 2 lines (summary + id), got: {}", stdout);
    assert!(lines[0].contains("Summary Title") && lines[0].contains("sum_list"), "first line: {}", lines[0]);
    let id: u32 = lines[1].parse().expect("second line is id");
    assert_eq!(id, 1);
}

#[tokio::test]
async fn cli_add_short_flags_json_reaches_server() {
    let (mut child, port, _dir, _base_url) = spawn_server();
    if !wait_health(&format!("http://127.0.0.1:{}", port)).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let out = std::process::Command::new(cli_bin())
        .args(["add", "-p", &port.to_string(), "-j", "short_list", "From short flags", ""])
        .output()
        .expect("run cli add -p -j");
    let _ = child.kill();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let obj: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(obj.get("list").and_then(|v| v.as_str()), Some("short_list"));
}
