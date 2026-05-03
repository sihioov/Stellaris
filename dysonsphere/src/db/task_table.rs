// dysonsphere/db/task_table.rs

use crate::error::Result;
use crate::message::TaskMessage;
use crate::status::TaskStatus;
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    Applied,
    Stale { actual: TaskStatus },
}

/// TaskTable Trait
#[async_trait]
pub trait TaskTable {
    async fn create(&self, task: TaskMessage) -> Result<()>;

    async fn fetch(&self, task_id: &str) -> Result<Option<TaskMessage>>;

    /// Single source of truth for status changes. Validates the lifecycle,
    /// applies the transition, and bumps `updated_at` atomically.
    async fn transition(
        &self,
        task_id: &str,
        expected: TaskStatus,
        next: TaskStatus,
    ) -> Result<TransitionOutcome>;

    /// Deprecated: use `TaskTable::transition`; will be removed in the next epic.
    #[deprecated(note = "use TaskTable::transition; will be removed in next epic")]
    async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<()>;

    async fn delete(&self, task_id: &str) -> Result<()>;

    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>>;

    async fn fetch_dispatched(&self) -> Result<Vec<TaskMessage>>;

    async fn fetch_processed(&self) -> Result<Vec<TaskMessage>>;
}
