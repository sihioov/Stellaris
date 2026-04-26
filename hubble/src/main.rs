mod scanner;

use dysonsphere::db::{FileTaskTable, TaskTable};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn discovery_id(discovery: &scanner::Discovery) -> String {
    let mut h = DefaultHasher::new();
    discovery.kind.hash(&mut h);
    discovery.title.hash(&mut h);
    discovery.description.hash(&mut h);
    format!("hubble-{:016x}", h.finish())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let tasks_path = std::env::var("TASKS_JSON_PATH").unwrap_or_else(|_| "tasks.json".into());
    let repo_path = match std::env::var("REPO_PATH") {
        Ok(p) => PathBuf::from(p),
        Err(_) => std::env::current_dir()?,
    };
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));
    let interval = Duration::from_secs(3600);

    loop {
        log::info!("[hubble] 스캔 시작: {}", repo_path.display());
        let findings = scanner::scan(&repo_path).await;

        for discovery in &findings {
            let id = discovery_id(discovery);
            let task = discovery.to_task_message(&id);
            match table.create(task).await {
                Ok(()) => log::info!("[hubble] 발견 등록: {}", discovery.title),
                Err(_) => {} // 동일 발견은 이미 등록됨 (dedup)
            }
        }

        log::info!("[hubble] 다음 스캔까지 {}초 대기", interval.as_secs());
        tokio::time::sleep(interval).await;
    }
}
