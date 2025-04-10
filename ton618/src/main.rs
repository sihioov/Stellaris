mod file;
mod message;
mod datasource;

use std::time::Duration;
use tokio::time::sleep;
use anyhow::Result;
use crate::file::FileDataSource;
use crate::datasource::TaskDataSource;
use crate::message::TaskMessage;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let datasource = FileDataSource::new("tasks.json");

    loop {
        log::info!("Checking for pending tasks...");

        // 미처리 Task 조회
        log::info!("Calling fetch_pending()...");
        let tasks: Vec<TaskMessage> = datasource.fetch_pending().await?;
        log::info!("fetch_pending() completed with {} tasks", tasks.len());

        for task in &tasks {
            log::info!("Got task: {:?}", task.task_id);
            // 여기에 메시지 큐 전송 또는 Worker 호출 코드가 들어갈 수 있음
        }

        // (선택) 테스트용으로 한 번 처리된 Task 마킹해보기
        for task in &tasks {
            datasource.mark_processed(&task.task_id).await?;
        }

        // 10초 후 재시도 (cron)
        sleep(Duration::from_secs(10)).await;
    }
}
