//! Todo list persistence: file-backed TodoStore and TodoRepository trait. Codec and repository
//! are submodules; this module owns list I/O, locking, and validation.

use rand::seq::SliceRandom;
use std::io::{Read, Write};
use std::path::PathBuf;
use tokio::fs;
use tokio::task::spawn_blocking;

mod codec;
mod lock;
mod repository;

pub use codec::{format_item, parse_content};
pub use repository::TodoRepository;

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
    InvalidListName(String),
    ListNameTooLong(usize),
    Io(String),
    Spawn(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::InvalidListName(msg) => write!(f, "{}", msg),
            StoreError::ListNameTooLong(max) => write!(f, "list_name must be at most {} characters", max),
            StoreError::Io(msg) => write!(f, "{}", msg),
            StoreError::Spawn(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StoreError {}

fn sanitize_list_name(name: &str) -> Option<String> {
    let s = name.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        Some(s.to_string())
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct TodoItem {
    pub id: u32,
    pub completed: bool,
    pub title: String,
    pub body: String,
}

pub struct TodoStore {
    base_dir: PathBuf,
}

impl TodoStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub const MAX_LIST_NAME_LEN: usize = crate::domain::MAX_LIST_NAME_LEN;

    fn path_for_list(&self, list_name: &str) -> Option<PathBuf> {
        sanitize_list_name(list_name).map(|name| self.base_dir.join(format!("{}.md", name)))
    }

    pub fn validate_list_name(list_name: &str) -> Result<String> {
        let s = sanitize_list_name(list_name)
            .ok_or_else(|| StoreError::InvalidListName("list_name must be non-empty and contain only letters, numbers, underscores, and hyphens".to_string()))?;
        if s.len() > Self::MAX_LIST_NAME_LEN {
            return Err(StoreError::ListNameTooLong(Self::MAX_LIST_NAME_LEN));
        }
        Ok(s)
    }

    pub fn list_exists(&self, list_name: &str) -> bool {
        self.path_for_list(list_name).map(|p| p.exists()).unwrap_or(false)
    }

    fn read_list_file_blocking(path: &std::path::Path) -> Result<String> {
        if !path.exists() {
            return Ok(String::new());
        }
        let f = std::fs::File::open(path).map_err(|e| StoreError::Io(format!("open todo file for read: {}", e)))?;
        fs2::FileExt::lock_shared(&f).map_err(|e| StoreError::Io(format!("lock todo file for read: {}", e)))?;
        let mut s = String::new();
        Read::read_to_string(&mut &f, &mut s).map_err(|e| StoreError::Io(format!("read todo file: {}", e)))?;
        Ok(s)
    }

    fn write_list_file_atomic(path: &std::path::Path, content: &str) -> Result<()> {
        let parent = path.parent().ok_or_else(|| StoreError::Io("no parent dir".to_string()))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| StoreError::Io(format!("create todo temp file: {}", e)))?;
        temp.write_all(content.as_bytes()).map_err(|e| StoreError::Io(format!("write todo file: {}", e)))?;
        temp.write_all(b"\n").map_err(|e| StoreError::Io(format!("write todo file: {}", e)))?;
        temp.as_file_mut().sync_all().map_err(|e| StoreError::Io(format!("sync todo file: {}", e)))?;
        temp.persist(path).map_err(|e| StoreError::Io(format!("atomic rename todo file: {}", e)))?;
        if let Some(p) = path.parent() {
            let dir = std::fs::File::open(p).map_err(|e| StoreError::Io(format!("open parent dir: {}", e)))?;
            dir.sync_all().map_err(|e| StoreError::Io(format!("sync parent dir: {}", e)))?;
        }
        Ok(())
    }

    async fn load(&self, list_name: &str) -> Result<Vec<TodoItem>> {
        let path = self
            .path_for_list(list_name)
            .ok_or_else(|| StoreError::InvalidListName("invalid list name".to_string()))?
            .clone();
        let content = spawn_blocking(move || Self::read_list_file_blocking(&path))
            .await
            .map_err(|e| StoreError::Spawn(e.to_string()))??;
        Ok(parse_content(&content))
    }

    pub async fn list_names(&self) -> Result<Vec<String>> {
        let mut names = std::collections::BTreeSet::new();
        let mut entries = fs::read_dir(&self.base_dir).await.map_err(|e| StoreError::Io(format!("read dir: {}", e)))?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| StoreError::Io(format!("read dir entry: {}", e)))? {
            let path = entry.path();
            if path.is_file()
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && path.extension().is_some_and(|e| e == "md")
                && let Some(sanitized) = sanitize_list_name(stem)
                && sanitized.len() <= Self::MAX_LIST_NAME_LEN
            {
                let expected_path = self.base_dir.join(format!("{}.md", sanitized));
                if path == expected_path {
                    names.insert(sanitized);
                }
            }
        }
        Ok(names.into_iter().collect())
    }

    /// Used via `TodoRepository` trait by the server handler.
    #[allow(dead_code)]
    pub async fn get_all(&self, list_name: &str) -> Result<Vec<TodoItem>> {
        self.load(list_name).await
    }

    async fn modify_list_exclusive<F, R>(&self, list_name: &str, f: F) -> Result<R>
    where
        F: FnOnce(&mut Vec<TodoItem>) -> R + Send + 'static,
        R: Send + 'static,
    {
        let list_name = Self::validate_list_name(list_name)?;
        let path = self.path_for_list(&list_name).ok_or_else(|| StoreError::InvalidListName("invalid list name".to_string()))?.clone();
        let parent = path.parent().ok_or_else(|| StoreError::Io("no parent".to_string()))?.to_path_buf();
        let lock_path = lock::lock_path_for_list(&parent, &list_name);
        spawn_blocking(move || {
            let _guard = lock::acquire_exclusive(&lock_path)?;
            let content = TodoStore::read_list_file_blocking(&path)?;
            let mut items = parse_content(&content);
            let r = f(&mut items);
            let out = items.iter().map(format_item).collect::<Vec<_>>().join("\n");
            TodoStore::write_list_file_atomic(&path, &out)?;
            Ok::<_, StoreError>(r)
        })
        .await
        .map_err(|e| StoreError::Spawn(e.to_string()))?
    }

    pub async fn add(&self, list_name: &str, title: String, body: String) -> Result<u32> {
        let list_name = Self::validate_list_name(list_name)?;
        self.modify_list_exclusive(&list_name, |items| {
            let next_id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
            items.push(TodoItem {
                id: next_id,
                completed: false,
                title,
                body,
            });
            next_id
        })
        .await
    }

    pub async fn complete(&self, list_name: &str, id: u32) -> Result<bool> {
        let list_name = Self::validate_list_name(list_name)?;
        self.modify_list_exclusive(&list_name, move |items| {
            let Some(item) = items.iter_mut().find(|i| i.id == id) else {
                return false;
            };
            if item.completed {
                return true;
            }
            item.completed = true;
            true
        })
        .await
    }

    pub async fn pick(&self, list_name: &str) -> Result<Option<TodoItem>> {
        let list_name = Self::validate_list_name(list_name)?;
        let items = self.load(&list_name).await?;
        let pending: Vec<_> = items.into_iter().filter(|t| !t.completed).collect();
        if pending.is_empty() {
            return Ok(None);
        }
        let mut rng = rand::thread_rng();
        Ok(pending.choose(&mut rng).cloned())
    }
}

pub fn format_todos_markdown(items: &[TodoItem]) -> String {
    items.iter().map(format_item).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn item_strategy() -> impl Strategy<Value = (u32, bool, String, String)> {
        (
            1u32..=10000,
            any::<bool>(),
            "[a-zA-Z0-9][a-zA-Z0-9 ]{0,49}",
            "[a-zA-Z0-9 ]{0,80}",
        )
    }

    proptest! {
        #[test]
        fn parse_format_roundtrip(items in prop::collection::vec(item_strategy(), 0..15)) {
            let todo_items: Vec<TodoItem> = items
                .into_iter()
                .map(|(id, completed, title, body)| TodoItem { id, completed, title, body })
                .collect();
            let formatted: String = todo_items.iter().map(format_item).collect::<Vec<_>>().join("\n");
            let parsed = parse_content(&formatted);
            assert_eq!(parsed.len(), todo_items.len(), "round-trip length");
            for (a, b) in parsed.iter().zip(todo_items.iter()) {
                assert_eq!(a.id, b.id);
                assert_eq!(a.completed, b.completed);
                assert_eq!(a.title.trim(), b.title.trim(), "title");
                let want_body: String = b.body.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n").trim_end().to_string();
                assert_eq!(a.body, want_body, "body");
            }
        }
    }
}
