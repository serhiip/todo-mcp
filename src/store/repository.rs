use async_trait::async_trait;

use super::{TodoItem, TodoStore, Result};

#[async_trait]
pub trait TodoRepository: Send + Sync {
    async fn list_exists(&self, list_name: &str) -> bool;
    async fn list_names(&self) -> Result<Vec<String>>;
    async fn get_all(&self, list_name: &str) -> Result<Vec<TodoItem>>;
    async fn add(&self, list_name: &str, title: String, body: String) -> Result<u32>;
    async fn complete(&self, list_name: &str, id: u32) -> Result<bool>;
    async fn pick(&self, list_name: &str) -> Result<Option<TodoItem>>;
}

#[async_trait]
impl TodoRepository for TodoStore {
    async fn list_exists(&self, list_name: &str) -> bool {
        TodoStore::list_exists(self, list_name)
    }

    async fn list_names(&self) -> Result<Vec<String>> {
        TodoStore::list_names(self).await
    }

    async fn get_all(&self, list_name: &str) -> Result<Vec<TodoItem>> {
        self.load(list_name).await
    }

    async fn add(&self, list_name: &str, title: String, body: String) -> Result<u32> {
        TodoStore::add(self, list_name, title, body).await
    }

    async fn complete(&self, list_name: &str, id: u32) -> Result<bool> {
        TodoStore::complete(self, list_name, id).await
    }

    async fn pick(&self, list_name: &str) -> Result<Option<TodoItem>> {
        TodoStore::pick(self, list_name).await
    }
}
