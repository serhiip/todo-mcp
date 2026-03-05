mod support;

use support::*;

#[tokio::test]
async fn server_health_responds() {
    let (mut child, _port, _dir, base_url) = spawn_server();
    if !wait_health(&base_url).await {
        let _ = child.kill();
        panic!("server did not become ready");
    }
    let _ = child.kill();
}

include!("integration/wait.rs");
include!("integration/resources.rs");
include!("integration/tools.rs");
include!("integration/concurrency.rs");
include!("integration/signals.rs");
include!("integration/cli.rs");
