#[allow(dead_code)]
#[cfg(any(test, feature = "rabbitmq-dispatch"))]
mod datasource;
#[allow(dead_code)]
#[cfg(any(test, feature = "rabbitmq-dispatch"))]
mod file;
#[cfg(feature = "nosql")]
mod nosql;
#[cfg(feature = "rdb")]
mod rdb;
mod scheduler;
#[allow(dead_code)]
#[cfg(all(feature = "rabbitmq-dispatch", feature = "file-dispatch"))]
mod task;

use crate::scheduler::Schedule;
use anyhow::Result;
use dysonsphere::db::task_table_file::FileTaskTable;
use dysonsphere::db::{TaskTable, TransitionOutcome};
use dysonsphere::status::TaskStatus;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::{env, ffi::OsString};
use tokio::time::sleep;

fn resolve_tasks_json_path(value: Option<OsString>) -> PathBuf {
    value
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tasks.json"))
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let tasks_path = resolve_tasks_json_path(env::var_os("TASKS_JSON_PATH"));
    log::info!(
        "Using task file from TASKS_JSON_PATH-compatible path: {}",
        tasks_path.display()
    );
    let table = Arc::new(FileTaskTable::new(tasks_path));

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
                    .transition(&task.task_id, TaskStatus::Pending, TaskStatus::Dispatched)
                    .await
                {
                    Ok(TransitionOutcome::Applied) => {}
                    Ok(TransitionOutcome::Stale { actual }) => log::warn!(
                        "Skip dispatch for {} because status changed before update (actual={:?})",
                        task.task_id,
                        actual
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn resolve_tasks_json_path_uses_env_value_when_present() {
        assert_eq!(
            resolve_tasks_json_path(Some(OsString::from("/tmp/tasks.json"))),
            PathBuf::from("/tmp/tasks.json")
        );
    }

    #[test]
    fn resolve_tasks_json_path_defaults_to_local_tasks_json() {
        assert_eq!(resolve_tasks_json_path(None), PathBuf::from("tasks.json"));
        assert_eq!(
            resolve_tasks_json_path(Some(OsString::from(""))),
            PathBuf::from("tasks.json")
        );
    }
}
