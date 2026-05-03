//! Task message struct.
//! Used ton618 → laniakea communication format.

use serde::{Deserialize, Serialize};
//use serde_json::Value;
use crate::error::{Result, StellarisError};
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

impl TaskMessage {
    /// Validates and applies a status transition while updating metadata.
    ///
    /// Same-status calls are idempotent and intentionally do not bump
    /// `updated_at`; callers that need stale detection should do it one layer up.
    pub fn transition_to(&mut self, next: TaskStatus, now: DateTime<Utc>) -> Result<()> {
        if self.meta.status == next {
            return Ok(());
        }

        if !self.meta.status.can_transition_to(&next) {
            return Err(StellarisError::InvalidStatusTransition {
                task_id: self.task_id.clone(),
                from: self.meta.status.clone(),
                to: next,
            });
        }

        self.meta.status = next;
        self.meta.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(status: TaskStatus) -> TaskMessage {
        TaskMessage {
            task_id: "T1".to_string(),
            task_type: TaskType::NewsA,
            payload: "payload".to_string(),
            meta: TaskMeta {
                status,
                ..TaskMeta::default()
            },
        }
    }

    #[test]
    fn transition_to_pending_to_dispatched_updates_status_and_timestamp() {
        let mut task = task(TaskStatus::Pending);
        let before = task.meta.updated_at;
        let now = before + chrono::Duration::seconds(1);

        task.transition_to(TaskStatus::Dispatched, now).unwrap();

        assert_eq!(task.meta.status, TaskStatus::Dispatched);
        assert_eq!(task.meta.updated_at, now);
    }

    #[test]
    fn transition_to_processed_from_pending_returns_invalid_transition() {
        let mut task = task(TaskStatus::Pending);
        let err = task
            .transition_to(TaskStatus::Processed, Utc::now())
            .unwrap_err();

        assert!(matches!(
            err,
            StellarisError::InvalidStatusTransition {
                from: TaskStatus::Pending,
                to: TaskStatus::Processed,
                ..
            }
        ));
    }

    #[test]
    fn transition_to_same_status_is_idempotent_and_does_not_bump_timestamp() {
        let mut task = task(TaskStatus::Pending);
        let before = task.meta.updated_at;
        let later = before + chrono::Duration::seconds(1);

        task.transition_to(TaskStatus::Pending, later).unwrap();

        assert_eq!(task.meta.status, TaskStatus::Pending);
        assert_eq!(task.meta.updated_at, before);
    }

    #[test]
    fn transition_to_terminal_then_any_other_returns_invalid_transition() {
        let mut task = task(TaskStatus::PendingReview);
        task.transition_to(TaskStatus::Processed, Utc::now())
            .unwrap();

        let err = task
            .transition_to(TaskStatus::Failed, Utc::now())
            .unwrap_err();

        assert!(matches!(
            err,
            StellarisError::InvalidStatusTransition {
                from: TaskStatus::Processed,
                to: TaskStatus::Failed,
                ..
            }
        ));
    }
}
