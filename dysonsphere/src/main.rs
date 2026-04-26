// main.rs

use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::error::Result;
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let file_table = FileTaskTable::new(PathBuf::from("tasks3.json"));

    let task = TaskMessage {
        task_id: "task_004".to_string(),
        task_type: TaskType::NewsA,
        payload: "Process important data".to_string(),
        meta: TaskMeta::default(),
    };

    file_table.create(task).await?;

    println!("Task inserted successfully!");

    Ok(())
}
