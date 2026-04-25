use crate::core::{AgentTask, CanopusResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedTask {
    pub backend_id: String,
}

pub trait TaskBackend {
    fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask>;
}
