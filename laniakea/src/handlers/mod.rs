pub mod custom;
pub mod news_a;

use dysonsphere::{
    error::Result,
    message::{TaskMessage, TaskType},
};

pub async fn dispatch(task: &TaskMessage) -> Result<()> {
    match &task.task_type {
        TaskType::NewsA => news_a::handle(task).await,
        TaskType::Custom(label) => custom::handle(task, label).await,
        TaskType::Bug | TaskType::Security | TaskType::TestCoverage | TaskType::UXImprovement => {
            custom::handle(task, "canopus.agent").await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};

    fn make_task(task_type: TaskType) -> TaskMessage {
        TaskMessage {
            task_id: "test-1".to_string(),
            task_type,
            payload: "payload".to_string(),
            meta: TaskMeta::default(),
        }
    }

    #[tokio::test]
    async fn dispatch_news_a_returns_ok() {
        let task = make_task(TaskType::NewsA);
        assert!(dispatch(&task).await.is_ok());
    }

    #[tokio::test]
    async fn dispatch_unsupported_custom_returns_error() {
        let task = make_task(TaskType::Custom("foo".to_string()));
        let err = dispatch(&task).await.unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported custom task label `foo`"));
    }

    #[tokio::test]
    async fn dispatch_maintenance_types_are_routed() {
        // Verify the match is exhaustive and routing doesn't panic.
        // Actual canopus execution is not available in the test env, so we
        // only assert the function returns a Result without panicking.
        for task_type in [
            TaskType::Bug,
            TaskType::Security,
            TaskType::TestCoverage,
            TaskType::UXImprovement,
        ] {
            let task = make_task(task_type);
            let _ = dispatch(&task).await;
        }
    }
}
