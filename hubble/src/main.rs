mod scanner;

use dysonsphere::db::{FileTaskTable, TaskTable};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    env_logger::init();

    let tasks_path = std::env::var("TASKS_JSON_PATH").unwrap_or_else(|_| "tasks.json".into());
    let repo_path = std::env::var("REPO_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));
    let interval = Duration::from_secs(3600); // 1시간

    loop {
        log::info!("[hubble] 스캔 시작: {}", repo_path.display());
        let findings = scanner::scan(&repo_path).await;

        for (i, discovery) in findings.iter().enumerate() {
            let id = format!(
                "hubble-{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                i
            );
            let task = discovery.to_task_message(&id);
            match table.create(task).await {
                Ok(()) => log::info!("[hubble] 발견 등록: {}", discovery.title),
                Err(e) => log::warn!("[hubble] 등록 실패: {e}"),
            }
        }

        log::info!("[hubble] 다음 스캔까지 {}초 대기", interval.as_secs());
        tokio::time::sleep(interval).await;
    }
}
