use crate::core::{AgentTask, CanopusError, CanopusResult};
use crate::ports::{SubmittedTask, TaskBackend};
use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use std::path::PathBuf;
use tokio::runtime::Runtime;

/// Synchronous adapter. `submit` drives its own Tokio runtime;
/// do not call from within an existing Tokio runtime context.
pub struct StellarisTaskBackend {
    table: FileTaskTable,
    runtime: Runtime,
}

impl StellarisTaskBackend {
    pub fn new(path: PathBuf) -> CanopusResult<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| CanopusError::Backend(err.to_string()))?;
        Ok(Self {
            table: FileTaskTable::new(path),
            runtime,
        })
    }
}

impl TaskBackend for StellarisTaskBackend {
    fn submit(&self, task: &AgentTask) -> CanopusResult<SubmittedTask> {
        let message = TaskMessage {
            task_id: task.id.clone(),
            task_type: TaskType::Custom("canopus.agent".to_string()),
            payload: task.to_backend_payload(),
            meta: TaskMeta::default(),
        };

        self.runtime
            .block_on(self.table.create(message))
            .map_err(|err| CanopusError::Backend(err.to_string()))?;

        Ok(SubmittedTask {
            backend_id: task.id.clone(),
        })
    }
}
