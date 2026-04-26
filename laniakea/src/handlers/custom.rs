use dysonsphere::{error::Result, message::TaskMessage};

pub async fn handle(task: &TaskMessage, label: &str) -> Result<()> {
    log::info!("[Custom:{}] task_id={}", label, task.task_id);
    Ok(())
}
