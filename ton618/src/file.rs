use crate::datasource::TaskDataSource;
use async_trait::async_trait;
use dysonsphere::error::{Result, StellarisError};
use dysonsphere::message::TaskMessage;
use dysonsphere::status::TaskStatus;
use serde_json::Value;
use std::fs::{self};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct FileDataSource {
    pub path: PathBuf,
    pub lock: Mutex<()>,
}

impl FileDataSource {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        FileDataSource {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }
}

#[async_trait]
impl TaskDataSource for FileDataSource {
    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>> {
        let _guard = self.lock.lock().unwrap();

        let raw = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::warn!("File not found: {:?}. Returning empty list.", self.path);

                return Ok(vec![]);
            }
            Err(e) => {
                return Err(StellarisError::IoError(format!(
                    "Failed to read file: {:?}: {}",
                    self.path, e
                )));
            }
        };

        let all_tasks: Vec<TaskMessage> = match serde_json::from_str(&raw) {
            Ok(tasks) => tasks,
            Err(e) => {
                return Err(StellarisError::SerdeError(format!(
                    "Failed to parse task JSON: {}",
                    e
                )))
            }
        };

        let pending_tasks: Vec<_> = all_tasks
            .into_iter()
            .filter(|task| !matches!(task.meta.status, TaskStatus::Processed))
            .collect();

        Ok(pending_tasks)
    }

    async fn mark_processed(&self, task_id: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap();

        let raw = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(e) => {
                return Err(StellarisError::IoError(format!(
                    "Failed to read file: {:?}: {}",
                    self.path, e
                )))
            }
        };

        let mut tasks: Vec<TaskMessage> = match serde_json::from_str(&raw) {
            Ok(tasks) => tasks,
            Err(e) => {
                return Err(StellarisError::SerdeError(format!(
                    "Failed to parse task JSON: {}",
                    e
                )))
            }
        };

        let mut found = false;
        for task in &mut tasks {
            if task.task_id == task_id {
                task.meta.status = TaskStatus::Processed;
                found = true;
                break;
            }
        }

        if !found {
            return Err(StellarisError::NotFound(format!(
                "Task with id {} not found",
                task_id
            )));
        }

        let updated = match serde_json::to_string_pretty(&tasks) {
            Ok(json) => json,
            Err(e) => {
                return Err(StellarisError::SerdeError(format!(
                    "Failed to serialize tasks: {}",
                    e
                )))
            }
        };

        match fs::write(&self.path, updated) {
            Ok(_) => Ok(()),
            Err(e) => Err(StellarisError::IoError(format!(
                "Failed to write to file: {:?}: {}",
                self.path, e
            ))),
        }
    }

    async fn get_task(&self, task_id: &str) -> Result<TaskMessage> {
        let _guard = self.lock.lock().unwrap();

        let raw = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(StellarisError::NotFound(format!(
                    "File not found: {:?}",
                    self.path
                )));
            }
            Err(e) => {
                return Err(StellarisError::IoError(format!(
                    "Failed to read file: {:?}: {}",
                    self.path, e
                )));
            }
        };

        let all_tasks: Vec<TaskMessage> = match serde_json::from_str(&raw) {
            Ok(tasks) => tasks,
            Err(e) => {
                return Err(StellarisError::SerdeError(format!(
                    "Failed to parse task JSON: {}",
                    e
                )))
            }
        };

        for task in all_tasks {
            if task.task_id == task_id {
                return Ok(task);
            }
        }

        Err(StellarisError::NotFound(format!(
            "Task with id {} not found",
            task_id
        )))
    }
}
