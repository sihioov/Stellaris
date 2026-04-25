use crate::core::CanopusResult;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait ToolGateway {
    fn ensure_clean_worktree(&self, repo: &Path) -> CanopusResult<()>;
    fn create_branch(&self, repo: &Path, branch: &str) -> CanopusResult<CommandOutput>;
    fn run_check(&self, repo: &Path, command: &[&str]) -> CanopusResult<CommandOutput>;
    fn changed_files(&self, repo: &Path) -> CanopusResult<CommandOutput>;
}
