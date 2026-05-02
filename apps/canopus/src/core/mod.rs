pub mod error;
pub mod pipeline;
pub mod run_identity;
pub mod types;
pub mod workflow;

pub use error::{CanopusError, CanopusResult};
pub use pipeline::Pipeline;
pub use run_identity::{derive_run_identity, sanitize_run_identity};
pub use types::{
    Agenda, AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind, GitHubIssueMetadata,
    StageRecord,
};
pub use workflow::WorkflowState;
