mod file;
mod datasource;
mod rdb;
mod nosql;
mod schedule;
mod scheduler;

use dysonsphere::message::TaskMessage;
use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;
use crate::file::FileDataSource;
use crate::datasource::TaskDataSource;
use crate::schedule::Scheduler;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let datasource = FileDataSource::new("tasks.json");

    loop {
        log::info!("Checking for pending tasks...");

        let tasks = datasource.fetch_pending().await?;

        if tasks.is_empty() {
            log::info!("⏸ No pending tasks.");
        } else {
            for task in &tasks {
                /// Task processing example
                /// send_to_laniakea(task).await;
                log::info!("Got task: {}", task.task_id);
            }

            for task in &tasks {
                datasource.mark_processed(&task.task_id).await?;
            }
        }

        let scheduler = Scheduler::fixed(Duration::from_secs(10));
        scheduler.wait_for_next().await;
    }

}
