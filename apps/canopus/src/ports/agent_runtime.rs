use crate::core::{AgentRunResult, AgentTask, CanopusResult};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub repo_path: PathBuf,
}

pub trait AgentRuntime {
    fn run(&self, task: &AgentTask, context: &AgentContext) -> CanopusResult<AgentRunResult>;
}
