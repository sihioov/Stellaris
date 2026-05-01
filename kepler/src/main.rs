mod scanner;

use dysonsphere::db::FileTaskTable;
use dysonsphere::discovery::register_discoveries;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let tasks_path = std::env::var("TASKS_JSON_PATH").unwrap_or_else(|_| "tasks.json".into());
    let repo_path = match std::env::var("REPO_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => std::env::current_dir()?,
    };
    let seen_path = std::env::var("KEPLER_SEEN_JSON_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_path.join(".canopus/kepler/seen.json"));
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));
    let interval = Duration::from_secs(3600);

    loop {
        log::info!("[kepler] 코드베이스 스캔 시작: {}", repo_path.display());
        let findings = scanner::scan(&repo_path).await;
        register_discoveries(table.as_ref(), &seen_path, &findings).await;

        log::info!("[kepler] 다음 스캔까지 {}초 대기", interval.as_secs());
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dysonsphere::db::TaskTable;
    use dysonsphere::discovery::Discovery as _;
    use dysonsphere::status::TaskStatus;
    use std::fs;

    #[tokio::test]
    async fn registers_code_discovery_once_as_pending_proposal() {
        let root = std::env::temp_dir().join(format!("kepler-seen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let tasks_path = root.join("tasks.json");
        let seen_path = root.join(".canopus/kepler/seen.json");
        let table = FileTaskTable::new(tasks_path);
        let discoveries = vec![scanner::Discovery {
            kind: scanner::DiscoveryKind::Bug,
            title: "Clippy 경고".to_string(),
            description: "warning: example".to_string(),
        }];

        assert_eq!(
            register_discoveries(&table, &seen_path, &discoveries).await,
            1
        );
        assert_eq!(
            register_discoveries(&table, &seen_path, &discoveries).await,
            0
        );

        let id = discoveries[0].id();
        let stored = table.fetch(&id).await.unwrap().unwrap();
        assert_eq!(stored.meta.status, TaskStatus::PendingProposal);
        assert!(seen_path.exists());
        let seen: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&seen_path).unwrap()).unwrap();
        let first_seen = seen[&id]["first_seen"].as_str().unwrap();
        assert!(!first_seen.starts_with("unix:"));
        chrono::DateTime::parse_from_rfc3339(first_seen).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
