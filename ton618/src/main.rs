// v1은 ton618의 file-backed scheduler/dispatcher만 사용한다. RabbitMQ
// dispatch (`task/dispatcher.rs`), cron-style scheduling (`scheduler/`),
// `nosql`/`rdb` 모듈은 v2까지 reserved 상태이므로 본 crate-wide allow를 둔다.
// Reserved surface가 활성화될 때(예: v2 dispatcher 도입 시) 본 allow를
// 제거해 unused 경로가 다시 dead_code lint에 잡히도록 해야 한다.
#![allow(dead_code, unused_imports, clippy::large_enum_variant)]

mod datasource;
mod file;
mod nosql;
mod rdb;
mod scheduler;
mod task;

use crate::scheduler::Schedule;
use anyhow::Result;
use dysonsphere::db::task_table_file::FileTaskTable;
use dysonsphere::db::TaskTable;
use dysonsphere::status::TaskStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let tasks_path = std::env::var("TASKS_JSON_PATH").unwrap_or_else(|_| "tasks.json".into());
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));

    loop {
        log::info!("Checking for pending tasks...");

        let tasks = table.fetch_pending().await.unwrap_or_else(|e| {
            log::error!("fetch_pending failed: {e}");
            vec![]
        });

        if tasks.is_empty() {
            log::info!("⏸ No pending tasks.");
        } else {
            for task in &tasks {
                log::info!(
                    "Dispatching task: {} (type: {:?})",
                    task.task_id,
                    task.task_type
                );
                match table
                    .update_status_if_current(
                        &task.task_id,
                        TaskStatus::Pending,
                        TaskStatus::Dispatched,
                    )
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => log::warn!(
                        "Skip dispatch for {} because status changed before update",
                        task.task_id
                    ),
                    Err(e) => log::error!("Failed to dispatch task {}: {e}", task.task_id),
                }
            }
        }

        let schedule = Schedule::fixed(Duration::from_secs(10));
        let delay = schedule.next_delay();
        sleep(delay).await;
    }
}
