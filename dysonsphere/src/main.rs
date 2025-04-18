// main.rs

use dysonsphere::db::{TaskTable, FileTaskTable};
use dysonsphere::message::{TaskMessage, TaskMeta};
use dysonsphere::error::{Result, StellarisError};
use std::path::PathBuf;
//use dysonsphere::status::TaskStatus;
//use dysonsphere::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // FileTaskTable 인스턴스 생성 (여기서는 "tasks.json" 파일 사용)
    let file_table = FileTaskTable::new(PathBuf::from("tasks.json"));

    // 새로운 TaskMessage 생성
    let task = TaskMessage {
        task_id: "task_001".to_string(),
        payload: "Process important data".to_string(),
        meta: TaskMeta::default(), // 기본 meta: Pending 상태와 현재 타임스탬프가 자동 설정됨
    };

    // create 메서드를 통해 새로운 Task를 삽입합니다.
    file_table.create(task).await?;

    println!("Task inserted successfully!");

    Ok(())
}
