use crate::status::TaskStatus;

#[derive(Debug, thiserror::Error)]
pub enum StellarisError {
    #[error("database error: {0}")]
    DatabaseError(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("serialization error: {0}")]
    SerdeError(String),
    #[error("MQ error: {0}")]
    MQError(String),
    #[error("JSON error: {0}")]
    JsonError(String),
    #[error("invalid status transition: {task_id} from {from:?} to {to:?}")]
    InvalidStatusTransition {
        task_id: String,
        from: TaskStatus,
        to: TaskStatus,
    },
    #[error("status conflict for {task_id}: expected {expected:?}, found {actual:?}")]
    StatusConflict {
        task_id: String,
        expected: TaskStatus,
        actual: TaskStatus,
    },
    #[error("duplicate task: {0}")]
    DuplicateTask(String),
    #[error("task not found: {0}")]
    TaskNotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Change to SetllarisError
impl From<std::io::Error> for StellarisError {
    fn from(err: std::io::Error) -> Self {
        StellarisError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for StellarisError {
    fn from(err: serde_json::Error) -> Self {
        StellarisError::SerdeError(err.to_string())
    }
}

impl From<lapin::Error> for StellarisError {
    fn from(error: lapin::Error) -> Self {
        StellarisError::MQError(format!("RabbitMQ error: {}", error))
    }
}

/// Result alias
pub type Result<T> = std::result::Result<T, StellarisError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_invalid_status_transition_includes_task_id_and_states() {
        let err = StellarisError::InvalidStatusTransition {
            task_id: "T1".to_string(),
            from: TaskStatus::Pending,
            to: TaskStatus::Processed,
        };
        let displayed = err.to_string();
        assert!(displayed.contains("T1"));
        assert!(displayed.contains("Pending"));
        assert!(displayed.contains("Processed"));
    }

    #[test]
    fn display_status_conflict_includes_expected_and_actual() {
        let err = StellarisError::StatusConflict {
            task_id: "T2".to_string(),
            expected: TaskStatus::Dispatched,
            actual: TaskStatus::Failed,
        };
        let displayed = err.to_string();
        assert!(displayed.contains("T2"));
        assert!(displayed.contains("Dispatched"));
        assert!(displayed.contains("Failed"));
    }

    #[test]
    fn display_duplicate_task_includes_id() {
        assert!(StellarisError::DuplicateTask("T3".to_string())
            .to_string()
            .contains("T3"));
    }

    #[test]
    fn from_io_error_maps_to_io_variant() {
        let err = StellarisError::from(std::io::Error::other("boom"));
        assert!(matches!(err, StellarisError::IoError(_)));
    }

    #[test]
    fn from_serde_error_maps_to_serde_variant() {
        let err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let err = StellarisError::from(err);
        assert!(matches!(err, StellarisError::SerdeError(_)));
    }
}
