use async_trait::async_trait;
use std::fs::{self};
use std::path::PathBuf;
use std::sync::Mutex;
use crate::datasource::TaskDataSource;
use anyhow::{Result, Context};
use serde_json::Value;
use dysonsphere::message::TaskMessage;

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
                return Ok(vec![]);  // 🔥 파일 없어도 처리 가능
            }
            Err(e) => {
                return Err(e).context(format!("failed to read file: {:?}", self.path));
            }
        };

        let all_tasks: Vec<TaskMessage> = serde_json::from_str(&raw)
            .with_context(|| "failed to parse task JSON")?;

        let pending_tasks = all_tasks
            .into_iter()
            .filter(|task| {
                if let Some(Value::String(status)) = task.meta.as_ref().and_then(|m| m.get("status")) {
                    status != "processed"
                } else {
                    true
                }
            })
            .collect();

        Ok(pending_tasks)
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
        fs::write(&self.path, new_data.as_bytes())?;

        Ok(())
    }
}
