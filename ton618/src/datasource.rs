use dysonsphere::error::Result;
use dysonsphere::message::TaskMessage;

#[async_trait::async_trait]
pub trait TaskDataSource: Send + Sync {
    /// Unprocessed task list
    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>>;

    /// Mark a specific task_id as completed processing.
    #[deprecated(
        note = "use the TaskTable lifecycle transition path; direct file processing is reserved"
    )]
    async fn mark_processed(&self, task_id: &str) -> Result<()>;

    /// Get a specific task by its ID
    async fn get_task(&self, task_id: &str) -> Result<TaskMessage>;
}
