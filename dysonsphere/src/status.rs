use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Dispatched,
    PendingReview,
    Processed,
    Failed,
    PendingProposal,
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, TaskStatus::Processed | TaskStatus::Failed)
    }

    pub fn is_dispatchable(&self) -> bool {
        matches!(self, TaskStatus::Pending)
    }

    pub fn can_transition_to(&self, next: &TaskStatus) -> bool {
        matches!(
            (self, next),
            (TaskStatus::Pending, TaskStatus::Dispatched)
                | (TaskStatus::Pending, TaskStatus::Failed)
                | (TaskStatus::Pending, TaskStatus::PendingProposal)
                | (TaskStatus::Dispatched, TaskStatus::PendingReview)
                | (TaskStatus::Dispatched, TaskStatus::Failed)
                | (TaskStatus::PendingReview, TaskStatus::Processed)
                | (TaskStatus::PendingReview, TaskStatus::Failed)
                | (TaskStatus::PendingProposal, TaskStatus::Pending)
                | (TaskStatus::PendingProposal, TaskStatus::Failed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::TaskStatus;

    #[test]
    fn pending_can_transition_to_dispatched_failed_proposal() {
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Dispatched));
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Failed));
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::PendingProposal));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Processed));
        assert!(TaskStatus::Pending.is_dispatchable());
    }

    #[test]
    fn dispatched_can_transition_to_review_or_failed_only() {
        assert!(TaskStatus::Dispatched.can_transition_to(&TaskStatus::PendingReview));
        assert!(TaskStatus::Dispatched.can_transition_to(&TaskStatus::Failed));
        assert!(!TaskStatus::Dispatched.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Dispatched.can_transition_to(&TaskStatus::Processed));
    }

    #[test]
    fn processed_is_terminal_and_rejects_non_idempotent() {
        assert!(TaskStatus::Processed.is_terminal());
        assert!(!TaskStatus::Processed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Processed.can_transition_to(&TaskStatus::Processed));
    }

    #[test]
    fn failed_is_terminal_and_rejects_non_idempotent() {
        assert!(TaskStatus::Failed.is_terminal());
        assert!(!TaskStatus::Failed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Failed.can_transition_to(&TaskStatus::Failed));
    }

    #[test]
    fn pending_review_can_transition_to_processed_or_failed_only() {
        assert!(TaskStatus::PendingReview.can_transition_to(&TaskStatus::Processed));
        assert!(TaskStatus::PendingReview.can_transition_to(&TaskStatus::Failed));
        assert!(!TaskStatus::PendingReview.can_transition_to(&TaskStatus::Dispatched));
    }

    #[test]
    fn pending_proposal_can_transition_to_pending_or_failed() {
        assert!(TaskStatus::PendingProposal.can_transition_to(&TaskStatus::Pending));
        assert!(TaskStatus::PendingProposal.can_transition_to(&TaskStatus::Failed));
        assert!(!TaskStatus::PendingProposal.can_transition_to(&TaskStatus::Dispatched));
    }
}
