use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskMessage {
    pub task_id: String,
    pub r#type: String,
    pub source: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
    pub meta: Option<serde_json::Value>,
}
