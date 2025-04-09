use async_trait::async_trait;
use crate::message::TaskMessage;

#[async_trait]
pub trait TaskDataSource {
    async fn fetch_pending(&self) -> anyhow::Result<Vec<TaskMessage>>;
    async fn mark_processed(&self, task_id: &str) -> anyhow::Result<()>;
}
