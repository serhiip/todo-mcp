//! Todo MCP server binary. Composes config, server (MCP tools/resources), and store;
//! process bootstrap only: load config, build HTTP/MCP service, run until shutdown.
//! CLI: `todo-mcp add <list_name> <title> [body]` calls the running MCP server to add a todo (list auto-create and id generation on server).

mod app;
mod cli;
mod config;
mod domain;
mod server;
mod store;

use tracing_subscriber::EnvFilter;

use axum::routing::get;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use app::{Config, TodoServer};

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let bin = args.first().map(String::as_str).unwrap_or("todo-mcp");
    if args.get(1).map(String::as_str) == Some("add") {
        let add_args = &args[2..];
        if add_args.iter().any(|a| a == "--help" || a == "-h") {
            cli::print_add_help(bin);
            return Ok(());
        }
        let opts = match cli::parse_add_args(add_args) {
            Ok(o) => o,
            Err(e) => {
                cli::report_add_error(&e);
                std::process::exit(cli::exit_code_for_error(&e));
            }
        };
        if opts.dry_run {
            cli::print_dry_run(&opts);
            return Ok(());
        }
        match cli::add_via_server(
            &opts.base_url,
            &opts.list_name,
            &opts.title,
            &opts.body,
            opts.timeout_secs,
        )
        .await
        {
            Ok(id) => {
                if opts.json {
                    let out = serde_json::json!({
                        "id": id,
                        "list": opts.list_name,
                        "title": opts.title,
                        "server": opts.base_url
                    });
                    let s = if opts.json_pretty && !opts.quiet {
                        serde_json::to_string_pretty(&out).unwrap()
                    } else {
                        serde_json::to_string(&out).unwrap()
                    };
                    println!("{}", s);
                } else if opts.summary {
                    cli::print_success_summary(id, &opts.list_name, &opts.title);
                } else {
                    cli::print_success_id(id);
                }
                return Ok(());
            }
            Err(e) => {
                cli::report_add_error(&e);
                std::process::exit(cli::exit_code_for_error(&e));
            }
        }
    }
    if args.get(1).map(String::as_str).map(|a| a == "--help" || a == "-h") == Some(true) {
        cli::print_add_help(bin);
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let config = Config::from_env()?;
    let addr = config.addr()?;
    let base_dir = config.base_dir.clone();
    let body_limit = config.body_limit_bytes;

    let service = StreamableHttpService::new(
        move || {
            TodoServer::new_with_base(base_dir.clone()).map_err(|e: anyhow::Error| std::io::Error::other(e.to_string()))
        },
        std::sync::Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new()
        .route("/health", get(health))
        .nest_service("/mcp", service.clone())
        .fallback_service(service)
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(body_limit));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("Todo MCP server at http://{} (MCP at /mcp, health at /health)", addr);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down gracefully"),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down gracefully"),
    }
}
