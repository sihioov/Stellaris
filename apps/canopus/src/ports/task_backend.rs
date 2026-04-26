use crate::core::{AgentTask, CanopusResult};
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedTask {
    pub backend_id: String,
}

#[async_trait]
pub trait TaskBackend {
    async fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask>;
}
