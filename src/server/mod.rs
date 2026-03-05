//! MCP server: tools, resources, and error mapping. Handler implements ServerHandler and tool router;
//! error module maps store errors to MCP responses.

mod error;
mod handler;
mod protocol;

pub use error::store_error_to_mcp;
pub use handler::TodoServer;
