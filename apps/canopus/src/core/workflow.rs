use crate::core::error::{CanopusError, CanopusResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowState {
    Created,
    Planned,
    Executing,
    Checking,
    Reviewed,
    Completed,
    Failed,
}

impl WorkflowState {
    pub fn transition_to(self, next: WorkflowState) -> CanopusResult<WorkflowState> {
        let allowed = matches!(
            (self, next),
            (WorkflowState::Created, WorkflowState::Planned)
                | (WorkflowState::Planned, WorkflowState::Executing)
                | (WorkflowState::Executing, WorkflowState::Checking)
                | (WorkflowState::Checking, WorkflowState::Reviewed)
                | (WorkflowState::Reviewed, WorkflowState::Completed)
        ) || (next == WorkflowState::Failed && self != WorkflowState::Failed);

        if allowed {
            Ok(next)
        } else {
            Err(CanopusError::InvalidTransition(format!("{self:?} -> {next:?}")))
        }
    }
}
