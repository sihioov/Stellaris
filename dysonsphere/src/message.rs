use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Task message struct.
/// Used ton618 → laniakea communication format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    /// Task ID (unique identifier)
    pub task_id: String,

    /// Body data (ex: A collected text, path, etc)
    pub payload: String,

    /// Additional information (Flexible use of status, time, tag, etc)
    pub meta: Option<Value>,
}
