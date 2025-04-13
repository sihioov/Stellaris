use anyhow::Result;
use dysonsphere::message::TaskMessage;

#[async_trait::async_trait]
pub trait TaskDataSource: Send + Sync {
    /// Unprocessed task list
    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>>;

    /// Mark a specific task_id as completed processing.
    async fn mark_processed(&self, task_id: &str) -> Result<()>;
}
