pub mod branch_naming;
pub mod commit_message;
pub mod error;
pub mod module_derivation;
pub mod pipeline;
pub mod run_identity;
pub mod types;
pub mod workflow;

pub use error::{CanopusError, CanopusResult};
pub use pipeline::Pipeline;
pub use run_identity::{derive_run_identity, sanitize_run_identity};
pub use types::{
    deterministic_agenda_id_for_github_issue, deterministic_agenda_id_for_github_project, Agenda,
    AgendaSource, AgentMessage, AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind,
    GitHubIssueMetadata, GitHubProjectMetadata, GitHubProjectMode, StageRecord,
};
pub use workflow::WorkflowState;
