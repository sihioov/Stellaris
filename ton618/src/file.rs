use async_trait::async_trait;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;
use std::io::Write;
use crate::message::TaskMessage;
use crate::datasource::TaskDataSource;
use anyhow::{Result, Context};

use serde_json::Value;

pub struct FileDataSource {
    pub path: PathBuf,
    pub lock: Mutex<()>,
}

#[async_trait]
impl TaskDataSource for FileDataSource {
    async fn fetch_pending(&self) -> Result<Vec<TaskMessage>> {
        let _guard = self.lock.lock().unwrap();

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read file: {:?}", self.path))?;

        let tasks: Vec<TaskMessage> = serde_json::from_str(&raw)?;
        let pending: Vec<TaskMessage> = tasks
            .into_iter()
            .filter(|t| {
                if let Some(Value::String(status)) = &t.meta.as_ref().and_then(|m| m.get("status")) {
                    status != "processed"
                } else {
                    true
                }
            })
            .collect();

        Ok(pending)
    }

    async fn mark_processed(&self, task_id: &str) -> Result<()> {
        let _guard = self.lock.lock().unwrap();

        let raw = fs::read_to_string(&self.path)?;
        let mut tasks: Vec<TaskMessage> = serde_json::from_str(&raw)?;

        for task in &mut tasks {
            if task.task_id == task_id {
                let mut meta = task.meta.take().unwrap_or_else(|| Value::Object(Default::default()));
                if let Value::Object(ref mut map) = meta {
                    map.insert("status".to_string(), Value::String("processed".to_string()));
                }
                task.meta = Some(meta);
            }
        }

        let new_data = serde_json::to_string_pretty(&tasks)?;
        let mut file = OpenOptions::new().write(true).truncate(true).open(&self.path)?;
        file.write_all(new_data.as_bytes())?;

        Ok(())
    }
}
