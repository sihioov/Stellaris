//! Task message struct.
//! Used ton618 → laniakea communication format.

use serde::{Deserialize, Serialize};
//use serde_json::Value;
use crate::status::TaskStatus;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum TaskType {
    NewsA,
    Custom(String),
    // 유지보수 모드 유형
    Bug,
    Security,
    TestCoverage,
    UXImprovement,
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
