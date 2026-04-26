use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use dysonsphere::{
    db::{task_table_file::FileTaskTable, TaskTable},
    error::Result,
    message::TaskMessage,
    status::TaskStatus,
};
use crate::handlers;

async fn process_task(task: &TaskMessage) -> Result<()> {
    handlers::dispatch(task).await
}

pub async fn run_file_loop(table: Arc<FileTaskTable>, interval: Duration) -> Result<()> {
    loop {
        let pending = match table.fetch_pending().await {
            Ok(tasks) => tasks,
            Err(e) => {
                log::error!("[file] fetch_pending failed: {e}");
                tokio::time::sleep(interval).await;
                continue;
            }
        };
        for task in pending {
            match process_task(&task).await {
                Ok(()) => {
                    if let Err(e) = table.update_status(&task.task_id, TaskStatus::Processed).await {
                        log::error!("[file] update_status(Processed) failed for {}: {e}", task.task_id);
                    }
                }
                Err(e) => {
                    log::error!("[file] handler error for {}: {e}", task.task_id);
                    if let Err(ue) = table.update_status(&task.task_id, TaskStatus::Failed).await {
                        log::error!("[file] update_status(Failed) failed for {}: {ue}", task.task_id);
                    }
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
}

pub async fn run_rabbit_loop(mut rx: Receiver<TaskMessage>) -> Result<()> {
    while let Some(task) = rx.recv().await {
        if let Err(e) = process_task(&task).await {
            log::error!("[rabbit] handler error for {}: {e}", task.task_id);
        }
        // no_ack=true (dysonsphere) — ack 불필요, status 갱신 없음 (MVP 한정)
    }
    Ok(())
}
