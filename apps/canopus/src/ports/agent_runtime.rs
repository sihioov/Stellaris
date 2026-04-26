use crate::core::{AgentRunResult, AgentTask, Artifact, CanopusResult};
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub repo_path: PathBuf,
}

#[async_trait]
pub trait AgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        context: &AgentContext,
        prior_artifacts: &[Artifact],
    ) -> CanopusResult<AgentRunResult>;
}
