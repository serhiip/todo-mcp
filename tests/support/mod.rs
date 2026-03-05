//! Shared integration-test helpers. Organized by: fixtures (server spawn, health),
//! rpc (MCP protocol headers, SSE parse, tool/resource calls), session (initialize lifecycle).

pub mod fixtures;
pub mod rpc;
pub mod session;

pub use fixtures::*;
pub use rpc::*;
pub use session::*;
