use serde::{Deserialize, Serialize};
//use serde_json::Value;
use chrono::{DateTime, Utc};
use crate::status::TaskStatus;
/// Task message struct.
/// Used ton618 → laniakea communication format.

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TaskType {
    NewsA,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for TaskMeta {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            updated_at: now,
            status: TaskStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMessage {
    pub task_id: String,
    pub task_type: TaskType,
    pub payload: String,
    pub meta: TaskMeta,
}

