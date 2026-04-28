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
