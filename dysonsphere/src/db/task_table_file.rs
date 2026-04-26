use std::path::PathBuf;
use std::sync::Mutex;
use std::fs;
use async_trait::async_trait;
use crate::error::{Result, StellarisError};
use crate::message::TaskMessage;
use crate::status::TaskStatus;
use crate::db::task_table::TaskTable;

pub struct FileTaskTable {
    path: PathBuf,
    lock: Mutex<()>
}

impl FileTaskTable {
    pub fn new(path: PathBuf) -> FileTaskTable {
        FileTaskTable {
            path: path.into(),
            lock: Mutex::new(())
        }
    }

    fn read_tasks(&self) -> Result<Vec<TaskMessage>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let data = fs::read(&self.path)?;
        let data = String::from_utf8_lossy(&data);
        if data.trim().is_empty() {
            return Ok(vec![]);
        }

        let tasks: Vec<TaskMessage> = serde_json::from_str(&data)?;

        Ok(tasks)
    }

    fn write_tasks(&self, tasks: &[TaskMessage]) -> Result<()> {
        let json = serde_json::to_string_pretty(tasks)?;   //< Todo errlog
        fs::write(&self.path, json)?;

        Ok(())
    }
}

#[async_trait]
impl TaskTable for FileTaskTable {
    async fn create(&self, task: TaskMessage) -> Result<()> {
        /// File access synchronization
        let _guard = self.lock.lock().unwrap();
        let mut tasks = self.read_tasks()?;

        /// Filtering same task id
        if tasks.iter().any(|t| t.task_id == task.task_id) {
            //return Err(Error::TaskAlreadyCreated(task.task_id));
            return Err(StellarisError::DefaultError);
        }

        tasks.push(task);
        self.write_tasks(&tasks)?;

        Ok(())
    }

    async fn fetch(&self, task_id: &str) -> Result<Option<TaskMessage>> {
        let _guard = self.lock.lock().unwrap();
        let tasks = self.read_tasks()?;

        Ok(tasks.iter().find(|t| t.task_id == task_id).cloned())
    }

    async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        let _guard = self.lock.lock().unwrap();
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
        let _guard = self.lock.lock().unwrap();
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
        let _guard = self.lock.lock().unwrap();
        let tasks = self.read_tasks()?;
        let pending_tasks = self.read_tasks()?.into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Pending))
            .collect();

        Ok(pending_tasks)
    }

    async fn fetch_dispatched(&self) -> Result<Vec<TaskMessage>> {
        let _guard = self.lock.lock().unwrap();
        let tasks = self.read_tasks()?;
        let dispatched_tasks = self.read_tasks()?.into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Dispatched))
            .collect();

        Ok(dispatched_tasks)
    }
}
