//! Test fixtures: spawn server process, wait for health.

pub fn spawn_server() -> (std::process::Child, u16, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let port = portpicker::pick_unused_port().expect("free port");
    let bin = std::env::var("CARGO_BIN_EXE_todo-mcp").unwrap_or_else(|_| "target/debug/todo-mcp".to_string());
    let child = std::process::Command::new(bin)
        .env("MCP_PORT", port.to_string())
        .env("MCP_BASE_DIR", dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn todo-mcp");
    let base_url = format!("http://127.0.0.1:{}", port);
    (child, port, dir, base_url)
}

pub async fn wait_health(base_url: &str) -> bool {
    let url = format!("{}/health", base_url);
    for _ in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Ok(resp) = reqwest::get(&url).await
            && resp.status().is_success()
        {
            return resp.text().await.map(|t| t == "ok").unwrap_or(false);
        }
    }
    false
}
