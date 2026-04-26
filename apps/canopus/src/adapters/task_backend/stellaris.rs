use crate::core::{AgentTask, CanopusError, CanopusResult};
use crate::ports::{SubmittedTask, TaskBackend};
use async_trait::async_trait;
use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use std::fs;
use std::path::PathBuf;

pub struct StellarisTaskBackend {
    table: FileTaskTable,
}

impl StellarisTaskBackend {
    pub fn new(path: PathBuf) -> CanopusResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        Ok(Self {
            table: FileTaskTable::new(path),
        })
    }
}

#[async_trait]
impl TaskBackend for StellarisTaskBackend {
    async fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask> {
        let message = TaskMessage {
            task_id: task.id.clone(),
            task_type: TaskType::Custom("canopus.agent".to_string()),
            payload: task.to_backend_payload(),
            meta: TaskMeta::default(),
        };

        self.table
            .create(message)
            .await
            .map_err(|err| CanopusError::Backend(err.to_string()))?;

        Ok(SubmittedTask {
            backend_id: task.id.clone(),
        })
    }
}
