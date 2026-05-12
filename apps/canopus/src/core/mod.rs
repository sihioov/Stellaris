pub mod branch_naming;
pub mod commit_message;
pub mod error;
pub mod module_derivation;
pub mod pipeline;
pub mod pre_run_helper;
pub mod run_identity;
pub mod runtime_registry;
pub mod types;
pub mod workflow;

pub use error::{CanopusError, CanopusResult};
pub use pipeline::Pipeline;
pub use pre_run_helper::{
    helper_artifact_task_id, select_pre_run_helpers, HelperOutput, HelperProvenance, HelperRequest,
    HelperSelection, PreRunHelperConfig, PreRunHelperFailurePolicy, PreRunHelperMode,
};
pub use run_identity::{derive_run_identity, sanitize_run_identity};
pub use runtime_registry::{
    BackendAttemptDirectiveSource, BackendKind, BackendSelection, BackendSelectionAttempt,
    BackendSelectionSource, PreparationPolicy, RuntimeCapability, RuntimeRegistry,
};
pub use types::{
    deterministic_agenda_id_for_github_issue, deterministic_agenda_id_for_github_project, Agenda,
    AgendaSource, AgentMessage, AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind,
    GitHubIssueMetadata, GitHubProjectMetadata, GitHubProjectMode, StageRecord, TokenUsage,
};
pub use workflow::WorkflowState;
