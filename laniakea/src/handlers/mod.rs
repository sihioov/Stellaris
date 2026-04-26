pub mod custom;
pub mod news_a;

use dysonsphere::{error::Result, message::{TaskMessage, TaskType}};

pub async fn dispatch(task: &TaskMessage) -> Result<()> {
    match &task.task_type {
        TaskType::NewsA => news_a::handle(task).await,
        TaskType::Custom(label) => custom::handle(task, label).await,
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
    async fn dispatch_custom_returns_ok() {
        let task = make_task(TaskType::Custom("foo".to_string()));
        assert!(dispatch(&task).await.is_ok());
    }
}
