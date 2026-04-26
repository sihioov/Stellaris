mod file;
mod datasource;
mod rdb;
mod nosql;
mod scheduler;
mod task;

use dysonsphere::db::task_table_file::FileTaskTable;
use dysonsphere::db::TaskTable;
use dysonsphere::status::TaskStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;
use crate::scheduler::Schedule;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let tasks_path = std::env::var("TASKS_JSON_PATH")
        .unwrap_or_else(|_| "tasks.json".into());
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));

    loop {
        log::info!("Checking for pending tasks...");

        let tasks = table.fetch_pending().await
            .unwrap_or_else(|e| { log::error!("fetch_pending failed: {e}"); vec![] });

        if tasks.is_empty() {
            log::info!("⏸ No pending tasks.");
        } else {
            for task in &tasks {
                log::info!("Dispatching task: {} (type: {:?})", task.task_id, task.task_type);
                if let Err(e) = table.update_status(&task.task_id, TaskStatus::Dispatched).await {
                    log::error!("Failed to dispatch task {}: {e}", task.task_id);
                }
            }
        }

        let schedule = Schedule::fixed(Duration::from_secs(10));
        let delay = schedule.next_delay();
        sleep(delay).await;
    }
}
