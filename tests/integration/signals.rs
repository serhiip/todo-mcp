// Integration tests for OS signal handling: server shuts down on Ctrl+C / SIGTERM.

#[cfg(unix)]
#[tokio::test]
async fn server_shuts_down_on_sigterm() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let pid = child.id();
    let status = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("run kill -TERM");
    assert!(status.success(), "kill -TERM should succeed");
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        if tokio::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("server did not exit within 5s after SIGTERM");
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.code().is_some(), "server should exit with code");
                break;
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                panic!("try_wait: {}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}
