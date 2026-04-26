use dysonsphere::{error::Result, message::TaskMessage};

pub async fn handle(task: &TaskMessage) -> Result<()> {
    log::info!(
        "[NewsA] task_id={} payload_len={}",
        task.task_id,
        task.payload.len()
    );
    Ok(())
}
