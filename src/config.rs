//! Startup configuration from environment (MCP_HOST, MCP_PORT, MCP_BASE_DIR, MCP_BODY_LIMIT_BYTES).
//! Validates base_dir and resolves listen address.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub base_dir: PathBuf,
    pub body_limit_bytes: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let port = std::env::var("MCP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);
        let host = std::env::var("MCP_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let base_dir = std::env::var("MCP_BASE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().expect("current_dir"));
        let body_limit_bytes = std::env::var("MCP_BODY_LIMIT_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2 * 1024 * 1024);
        let config = Self {
            host,
            port,
            base_dir,
            body_limit_bytes,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !self.base_dir.exists() {
            std::fs::create_dir_all(&self.base_dir).map_err(|e| {
                anyhow::anyhow!(
                    "MCP_BASE_DIR {:?} does not exist and could not be created: {}",
                    self.base_dir,
                    e
                )
            })?;
        }
        if !self.base_dir.is_dir() {
            anyhow::bail!("MCP_BASE_DIR {:?} is not a directory", self.base_dir);
        }
        Ok(())
    }

    pub fn addr(&self) -> anyhow::Result<std::net::SocketAddr> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| anyhow::anyhow!("MCP_HOST/MCP_PORT: {}", e))
    }
}
