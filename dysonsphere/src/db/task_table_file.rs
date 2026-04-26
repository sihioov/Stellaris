use crate::db::task_table::TaskTable;
use crate::error::{Result, StellarisError};
use crate::message::TaskMessage;
use crate::status::TaskStatus;
use async_trait::async_trait;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

pub struct FileTaskTable {
    path: PathBuf,
}

impl FileTaskTable {
    pub fn new(path: PathBuf) -> FileTaskTable {
        FileTaskTable { path: path.into() }
    }

    fn read_tasks(&self) -> Result<Vec<TaskMessage>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let file = OpenOptions::new().read(true).open(&self.path)?;
        fs2::FileExt::lock_shared(&file)?;

        let mut content = String::new();
        (&file).read_to_string(&mut content)?;
        // lock released when file is dropped

        if content.trim().is_empty() {
            return Ok(vec![]);
        }

        let tasks: Vec<TaskMessage> = serde_json::from_str(&content)?;
        Ok(tasks)
    }

    fn write_tasks(&self, tasks: &[TaskMessage]) -> Result<()> {
        let json = serde_json::to_string_pretty(tasks)?;
        let tmp_path = self.path.with_extension("tmp");

        {
            let file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;
            fs2::FileExt::lock_exclusive(&file)?;
            (&file).write_all(json.as_bytes())?;
            // lock released here when file is dropped
        }

        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

#[async_trait]
impl TaskTable for FileTaskTable {
    async fn create(&self, task: TaskMessage) -> Result<()> {
        let mut tasks = self.read_tasks()?;

        if tasks.iter().any(|t| t.task_id == task.task_id) {
            return Err(StellarisError::DefaultError);
        }

        tasks.push(task);
        self.write_tasks(&tasks)?;
        Ok(())
    }

    async fn fetch(&self, task_id: &str) -> Result<Option<TaskMessage>> {
        let tasks = self.read_tasks()?;
        Ok(tasks.into_iter().find(|t| t.task_id == task_id))
    }

    async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        let mut tasks = self.read_tasks()?;
        let mut found = false;

        for task in tasks.iter_mut() {
            if task.task_id == task_id {
                task.meta.status = status.clone();
                found = true;
                break;
            }
        }

        if !found {
            return Err(StellarisError::DefaultError);
        }

        self.write_tasks(&tasks)?;
        Ok(())
    }

    async fn delete(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.read_tasks()?;
        let initial_len = tasks.len();
        tasks.retain(|t| t.task_id != task_id);
        if tasks.len() == initial_len {
            return Err(StellarisError::DefaultError);
        }
        self.write_tasks(&tasks)?;
        Ok(())
    }

    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>> {
        let pending_tasks = self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Pending))
            .collect();
        Ok(pending_tasks)
    }

    async fn fetch_dispatched(&self) -> Result<Vec<TaskMessage>> {
        let dispatched_tasks = self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Dispatched))
            .collect();
        Ok(dispatched_tasks)
    }

    async fn fetch_processed(&self) -> Result<Vec<TaskMessage>> {
        let processed_tasks = self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Processed))
            .collect();
        Ok(processed_tasks)
    }
}
