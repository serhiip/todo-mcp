use rmcp::ErrorData as McpError;

use crate::store::StoreError;

pub fn store_error_to_mcp(e: StoreError) -> McpError {
    match &e {
        StoreError::InvalidListName(_) | StoreError::ListNameTooLong(_) => McpError::invalid_request(e.to_string(), None),
        StoreError::Io(_) | StoreError::Spawn(_) => McpError::internal_error("store_error", Some(serde_json::json!({ "error": e.to_string() }))),
    }
}
