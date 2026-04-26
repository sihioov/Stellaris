// dysonsphere/db/task_table.rs

use async_trait::async_trait;
use crate::message::TaskMessage;
use crate::error::Result;
use crate::status::TaskStatus;

/// TaskTable Trait
#[async_trait]
pub trait TaskTable {
    async fn create(&self, task: TaskMessage) -> Result<()>;

    async fn fetch(&self, task_id: &str) -> Result<Option<TaskMessage>>;

    async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<()>;

    async fn delete(&self, task_id: &str) -> Result<()>;

    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>>;

    async fn fetch_dispatched(&self) -> Result<Vec<TaskMessage>>;
}
