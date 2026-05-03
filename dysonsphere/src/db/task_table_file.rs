use crate::db::task_table::TaskTable;
use crate::error::{Result, StellarisError};
use crate::message::TaskMessage;
use crate::status::TaskStatus;
use async_trait::async_trait;
use chrono::Utc;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct FileTaskTable {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileTaskTable {
    pub fn new(path: PathBuf) -> FileTaskTable {
        FileTaskTable {
            path,
            lock: Mutex::new(()),
        }
    }

    /// Read-modify-write under both the intra-process mutex and a cross-process
    /// exclusive file lock on `<path>.lock`, then atomically rename the result.
    fn modify<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Vec<TaskMessage>) -> Result<()>,
    {
        let _guard = self.lock.lock().unwrap();

        let lock_path = self.path.with_extension("lock");
        let lock_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        fs2::FileExt::lock_exclusive(&lock_file)?;

        let mut tasks = self.read_tasks_raw()?;
        f(&mut tasks)?;
        self.write_tasks_raw(&tasks)
        // lock_file dropped here → fs2 exclusive lock released
        // _guard dropped here → intra-process mutex released
    }

    /// Read without acquiring any lock; only call when the caller already holds
    /// the cross-process exclusive lock via `modify`.
    fn read_tasks_raw(&self) -> Result<Vec<TaskMessage>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let content = fs::read_to_string(&self.path)?;
        if content.trim().is_empty() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_str(&content)?)
    }

    /// Read with a cross-process shared lock for consistent reads.
    fn read_tasks(&self) -> Result<Vec<TaskMessage>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }
        let file = OpenOptions::new().read(true).open(&self.path)?;
        fs2::FileExt::lock_shared(&file)?;
        let mut content = String::new();
        (&file).read_to_string(&mut content)?;
        if content.trim().is_empty() {
            return Ok(vec![]);
        }
        Ok(serde_json::from_str(&content)?)
    }

    fn write_tasks_raw(&self, tasks: &[TaskMessage]) -> Result<()> {
        let json = serde_json::to_string_pretty(tasks)?;
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, json.as_bytes())?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

#[async_trait]
impl TaskTable for FileTaskTable {
    async fn create(&self, task: TaskMessage) -> Result<()> {
        self.modify(|tasks| {
            if tasks.iter().any(|t| t.task_id == task.task_id) {
                return Err(StellarisError::DefaultError);
            }
            tasks.push(task);
            Ok(())
        })
    }

    async fn fetch(&self, task_id: &str) -> Result<Option<TaskMessage>> {
        let tasks = self.read_tasks()?;
        Ok(tasks.into_iter().find(|t| t.task_id == task_id))
    }

    async fn update_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        self.modify(
            |tasks| match tasks.iter_mut().find(|t| t.task_id == task_id) {
                Some(t) => {
                    let now = Utc::now();
                    t.meta.status = status;
                    t.meta.updated_at = now;
                    Ok(())
                }
                None => Err(StellarisError::DefaultError),
            },
        )
    }

    async fn update_status_if_current(
        &self,
        task_id: &str,
        current: TaskStatus,
        next: TaskStatus,
    ) -> Result<bool> {
        let mut updated = false;
        self.modify(
            |tasks| match tasks.iter_mut().find(|t| t.task_id == task_id) {
                Some(t) if t.meta.status == current => {
                    let now = Utc::now();
                    t.meta.status = next;
                    t.meta.updated_at = now;
                    updated = true;
                    Ok(())
                }
                Some(_) => Ok(()),
                None => Err(StellarisError::DefaultError),
            },
        )?;
        Ok(updated)
    }

    async fn delete(&self, task_id: &str) -> Result<()> {
        self.modify(|tasks| {
            let initial_len = tasks.len();
            tasks.retain(|t| t.task_id != task_id);
            if tasks.len() == initial_len {
                Err(StellarisError::DefaultError)
            } else {
                Ok(())
            }
        })
    }

    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>> {
        Ok(self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Pending))
            .collect())
    }

    async fn fetch_dispatched(&self) -> Result<Vec<TaskMessage>> {
        Ok(self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Dispatched))
            .collect())
    }

    async fn fetch_processed(&self) -> Result<Vec<TaskMessage>> {
        Ok(self
            .read_tasks()?
            .into_iter()
            .filter(|t| matches!(t.meta.status, TaskStatus::Processed))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{TaskMeta, TaskType};

    fn task(id: &str, status: TaskStatus) -> TaskMessage {
        TaskMessage {
            task_id: id.to_string(),
            task_type: TaskType::NewsA,
            payload: format!("payload-{id}"),
            meta: TaskMeta {
                status,
                ..TaskMeta::default()
            },
        }
    }

    #[tokio::test]
    async fn update_status_if_current_does_not_resurrect_changed_tasks() {
        let path = std::env::temp_dir().join(format!(
            "dysonsphere-cas-status-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let table = FileTaskTable::new(path.clone());
        table
            .create(task("T1", TaskStatus::Dispatched))
            .await
            .unwrap();

        assert!(table
            .update_status_if_current("T1", TaskStatus::Dispatched, TaskStatus::PendingReview)
            .await
            .unwrap());
        assert!(!table
            .update_status_if_current("T1", TaskStatus::Dispatched, TaskStatus::Failed)
            .await
            .unwrap());
        let stored = table.fetch("T1").await.unwrap().unwrap();
        assert_eq!(stored.meta.status, TaskStatus::PendingReview);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn update_status_bumps_updated_at() {
        let path = std::env::temp_dir().join(format!(
            "dysonsphere-update-status-ts-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let table = FileTaskTable::new(path.clone());
        table.create(task("T1", TaskStatus::Pending)).await.unwrap();
        let before = table.fetch("T1").await.unwrap().unwrap().meta.updated_at;

        table
            .update_status("T1", TaskStatus::Dispatched)
            .await
            .unwrap();

        let stored = table.fetch("T1").await.unwrap().unwrap();
        assert_eq!(stored.meta.status, TaskStatus::Dispatched);
        assert!(stored.meta.updated_at > before);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn update_status_if_current_bumps_updated_at_only_on_success() {
        let path = std::env::temp_dir().join(format!(
            "dysonsphere-cas-status-ts-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let table = FileTaskTable::new(path.clone());
        table
            .create(task("T1", TaskStatus::Dispatched))
            .await
            .unwrap();
        let before = table.fetch("T1").await.unwrap().unwrap().meta.updated_at;

        assert!(table
            .update_status_if_current("T1", TaskStatus::Dispatched, TaskStatus::PendingReview)
            .await
            .unwrap());
        let after_success = table.fetch("T1").await.unwrap().unwrap().meta.updated_at;
        assert!(after_success > before);

        assert!(!table
            .update_status_if_current("T1", TaskStatus::Dispatched, TaskStatus::Failed)
            .await
            .unwrap());
        let after_stale = table.fetch("T1").await.unwrap().unwrap();
        assert_eq!(after_stale.meta.status, TaskStatus::PendingReview);
        assert_eq!(after_stale.meta.updated_at, after_success);
        let _ = fs::remove_file(path);
    }
}
