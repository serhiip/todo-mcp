use std::{collections::HashMap, sync::Arc};

use scopeguard::guard;
use rmcp::{
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    tool, tool_handler, tool_router,
    ErrorData as McpError,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::{watch, Mutex};
use tokio::time::{sleep, Duration};

use crate::domain::{MAX_BODY_LEN, MAX_TITLE_LEN};
use crate::server::store_error_to_mcp;
use crate::server::protocol::TODO_LIST_URI_PREFIX;
use crate::store::{format_todos_markdown, TodoRepository, TodoStore};

type WaiterList = Vec<(u64, watch::Sender<u64>)>;

struct WaiterRegistry {
    next_id: Mutex<u64>,
    waiters: Mutex<HashMap<String, WaiterList>>,
}

impl WaiterRegistry {
    fn new() -> Self {
        Self {
            next_id: Mutex::new(0),
            waiters: Mutex::new(HashMap::new()),
        }
    }

    async fn allocate_id(&self) -> u64 {
        let mut uid = self.next_id.lock().await;
        *uid = uid.wrapping_add(1);
        *uid
    }

    async fn add_waiter(&self, list_name: String, id: u64, sender: watch::Sender<u64>) {
        let mut waiters = self.waiters.lock().await;
        waiters.entry(list_name).or_default().push((id, sender));
    }

    async fn remove_waiter(&self, list_name: &str, id: u64) {
        let mut waiters = self.waiters.lock().await;
        if let Some(vec) = waiters.get_mut(list_name) {
            vec.retain(|(i, _)| *i != id);
            if vec.is_empty() {
                waiters.remove(list_name);
            }
        }
    }

    fn senders_for_list(waiters: &HashMap<String, WaiterList>, list_name: &str) -> Vec<watch::Sender<u64>> {
        waiters
            .get(list_name)
            .map(|v| v.iter().map(|(_, s)| s.clone()).collect())
            .unwrap_or_default()
    }

    async fn register(&self, list_name: String) -> (u64, watch::Receiver<u64>) {
        let (sender, rx) = watch::channel(0u64);
        let id = self.allocate_id().await;
        self.add_waiter(list_name, id, sender.clone()).await;
        (id, rx)
    }

    async fn unregister(&self, list_name: &str, id: u64) {
        self.remove_waiter(list_name, id).await;
    }

    async fn notify(&self, list_name: &str) {
        let senders = {
            let waiters = self.waiters.lock().await;
            Self::senders_for_list(&waiters, list_name)
        };
        for sender in senders {
            sender.send_modify(|v| *v = v.wrapping_add(1));
        }
    }
}

#[derive(Clone)]
pub struct TodoServer {
    store: Arc<Mutex<dyn TodoRepository + Send>>,
    waiters: Arc<WaiterRegistry>,
    tool_router: ToolRouter<TodoServer>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddTodoParams {
    list_name: String,
    title: String,
    body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CompleteTodoParams {
    list_name: String,
    id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PickTodoParams {
    list_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WaitForUpdateParams {
    list_name: String,
}

pub struct TodoServerBuilder {
    base_dir: std::path::PathBuf,
}

impl TodoServerBuilder {
    pub fn new(base_dir: std::path::PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn build(self) -> anyhow::Result<TodoServer> {
        let store: Arc<Mutex<dyn TodoRepository + Send>> =
            Arc::new(Mutex::new(TodoStore::new(self.base_dir)));
        Ok(TodoServer {
            store,
            waiters: Arc::new(WaiterRegistry::new()),
            tool_router: TodoServer::tool_router(),
        })
    }
}

#[tool_router]
impl TodoServer {
    pub fn new_with_base(base_dir: std::path::PathBuf) -> anyhow::Result<Self> {
        TodoServerBuilder::new(base_dir).build()
    }

    fn todo_list_resource(name: &str) -> Resource {
        let uri = format!("{}{}", TODO_LIST_URI_PREFIX, name);
        RawResource::new(uri.clone(), name)
            .with_description(format!("Todo list: {}", name))
            .with_mime_type("text/markdown")
            .no_annotation()
    }

    async fn notify_list_updated(&self, list_name: &str) {
        self.waiters.notify(list_name).await;
    }

    async fn list_snapshot(&self, list_name: &str) -> Result<String, McpError> {
        let store = self.store.lock().await;
        let items = store
            .get_all(list_name)
            .await
            .map_err(store_error_to_mcp)?;
        Ok(format_todos_markdown(&items))
    }

    #[tool(name = "add-todo", description = "Add a new todo item to a named list (list_name, title, body)")]
    async fn add_todo(
        &self,
        Parameters(p): Parameters<AddTodoParams>,
        peer: rmcp::Peer<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let AddTodoParams {
            list_name,
            title,
            body,
        } = p;
        let list_name = TodoStore::validate_list_name(&list_name)
            .map_err(store_error_to_mcp)?;
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(McpError::invalid_request("title must be non-empty", None));
        }
        if title.len() > MAX_TITLE_LEN {
            return Err(McpError::invalid_request(
                format!("title must be at most {} characters", MAX_TITLE_LEN),
                None,
            ));
        }
        let body = body.trim().to_string();
        if body.len() > MAX_BODY_LEN {
            return Err(McpError::invalid_request(
                format!("body must be at most {} characters", MAX_BODY_LEN),
                None,
            ));
        }
        let id = {
            let store = self.store.lock().await;
            store
                .add(&list_name, title.clone(), body)
                .await
                .map_err(store_error_to_mcp)?
        };
        self.notify_list_updated(&list_name).await;
        let uri = format!("{}{}", TODO_LIST_URI_PREFIX, list_name);
        let _ = peer.notify_resource_updated(ResourceUpdatedNotificationParam::new(uri)).await;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Added todo #{} to '{}': {}",
            id, list_name, title
        ))]))
    }

    #[tool(name = "complete-todo", description = "Mark a todo as complete by id in a named list (id is returned by add-todo)")]
    async fn complete_todo(
        &self,
        Parameters(p): Parameters<CompleteTodoParams>,
        peer: rmcp::Peer<rmcp::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let list_name = TodoStore::validate_list_name(&p.list_name)
            .map_err(store_error_to_mcp)?;
        let id = p.id;
        let ok = {
            let store = self.store.lock().await;
            store
                .complete(&list_name, id)
                .await
                .map_err(store_error_to_mcp)?
        };
        if ok {
            self.notify_list_updated(&list_name).await;
            let uri = format!("{}{}", TODO_LIST_URI_PREFIX, list_name);
            let _ = peer.notify_resource_updated(ResourceUpdatedNotificationParam::new(uri)).await;
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Marked todo #{} in '{}' as complete",
                id, list_name
            ))]))
        } else {
            Err(McpError::invalid_request(
                format!("Todo with id {} not found in list '{}'", id, list_name),
                None,
            ))
        }
    }

    #[tool(name = "pick-todo", description = "Pick a random pending todo from a named list")]
    async fn pick_todo(&self, Parameters(p): Parameters<PickTodoParams>) -> Result<CallToolResult, McpError> {
        let list_name = TodoStore::validate_list_name(&p.list_name)
            .map_err(store_error_to_mcp)?;
        let store = self.store.lock().await;
        let picked = store
            .pick(&list_name)
            .await
            .map_err(store_error_to_mcp)?;
        match picked {
            Some(item) => {
                let text = if item.body.is_empty() {
                    format!("#{} {}", item.id, item.title)
                } else {
                    format!("#{} {}\n\n{}", item.id, item.title, item.body)
                };
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Pick from '{}': {} (use id {} to complete)",
                    list_name, text, item.id
                ))]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No pending todos in '{}'.",
                list_name
            ))]))
        }
    }

    #[tool(name = "wait-for-update", description = "Wait until a named list is updated, then return")]
    async fn wait_for_update(
        &self,
        Parameters(p): Parameters<WaitForUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let list_name = TodoStore::validate_list_name(&p.list_name)
            .map_err(store_error_to_mcp)?;
        let poll_ms = std::env::var("MCP_WAIT_POLL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(500);
        let (id, mut rx) = self.waiters.register(list_name.clone()).await;
        let waiters = self.waiters.clone();
        let list_name_guard = list_name.clone();
        let _cleanup = guard((), move |_| {
            tokio::spawn(async move {
                waiters.unregister(&list_name_guard, id).await;
            });
        });
        let mut baseline = self.list_snapshot(&list_name).await?;
        loop {
            tokio::select! {
                changed = rx.changed() => {
                    changed.map_err(|_| McpError::internal_error("update_channel_closed", None))?;
                }
                _ = sleep(Duration::from_millis(poll_ms)) => {}
            }
            let current = self.list_snapshot(&list_name).await?;
            if current != baseline {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "List '{}' updated",
                    list_name
                ))]));
            }
            baseline = current;
        }
    }
}

#[tool_handler]
impl rmcp::ServerHandler for TodoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(
            "Todo list MCP server. Each folder can have multiple lists; every tool takes a list_name. Tools: add-todo (list_name, title, body) returns new todo id; complete-todo (list_name, id); pick-todo (list_name); wait-for-update (list_name). Resources: todo://list/{name} per list."
                .to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let store = self.store.lock().await;
        let names = store
            .list_names()
            .await
            .map_err(store_error_to_mcp)?;
        let resources = names
            .iter()
            .map(|n| Self::todo_list_resource(n))
            .collect::<Vec<_>>();
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let list_name = request
            .uri
            .strip_prefix(TODO_LIST_URI_PREFIX)
            .filter(|s| !s.is_empty() && !s.contains('/'));
        let list_name = match list_name {
            Some(n) => n,
            None => {
                return Err(McpError::resource_not_found(
                    "resource_not_found",
                    Some(serde_json::json!({ "uri": request.uri })),
                ));
            }
        };
        if TodoStore::validate_list_name(list_name).is_err() {
            return Err(McpError::resource_not_found(
                "resource_not_found",
                Some(serde_json::json!({ "uri": request.uri })),
            ));
        }
        let store = self.store.lock().await;
        if !store.list_exists(list_name).await {
            return Err(McpError::resource_not_found(
                "resource_not_found",
                Some(serde_json::json!({ "uri": request.uri })),
            ));
        }
        let items = store
            .get_all(list_name)
            .await
            .map_err(store_error_to_mcp)?;
        let text = format_todos_markdown(&items);
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            text,
            request.uri,
        )]))
    }
}
