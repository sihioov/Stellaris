mod scanner;

use dysonsphere::db::{FileTaskTable, TaskTable};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn discovery_id(discovery: &scanner::Discovery) -> String {
    let mut h = 0xcbf29ce484222325u64;
    stable_hash_part(&mut h, format!("{:?}", discovery.kind).as_bytes());
    stable_hash_part(&mut h, discovery.title.as_bytes());
    stable_hash_part(&mut h, discovery.description.as_bytes());
    format!("hubble-{h:016x}")
}

fn stable_hash_part(hash: &mut u64, bytes: &[u8]) {
    const FNV_PRIME: u64 = 0x100000001b3;
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn now_marker() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format!("unix:{secs}")
}

fn read_seen(path: &Path) -> Map<String, Value> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<Map<String, Value>>(&raw).unwrap_or_default(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(e) => {
            log::warn!("[hubble] seen ledger read failed {}: {e}", path.display());
            Map::new()
        }
    }
}

fn write_seen(path: &Path, seen: &Map<String, Value>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(seen)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        let _ = ureq::post(&url).send_json(body);
    }
}

async fn register_discoveries<T: TaskTable + Sync>(
    table: &T,
    seen_path: &Path,
    discoveries: &[scanner::Discovery],
) -> usize {
    let mut seen = read_seen(seen_path);
    let mut created = 0usize;

    for discovery in discoveries {
        let id = discovery_id(discovery);
        if seen.contains_key(&id) {
            log::info!("[hubble] 이미 본 후보 생략: {}", discovery.title);
            continue;
        }

        let task = discovery.to_task_message(&id);
        let mut should_mark_seen = false;
        match table.create(task).await {
            Ok(()) => {
                created += 1;
                should_mark_seen = true;
                log::info!("[hubble] 후보 등록: {}", discovery.title);
                notify_discord(&format!(
                    "📝 새 후보 발견: {} — `!propose-approve {}`",
                    discovery.title, id
                ));
            }
            Err(e) => match table.fetch(&id).await {
                Ok(Some(_)) => {
                    should_mark_seen = true;
                    log::info!("[hubble] 이미 등록된 후보 ledger 복구: {}", discovery.title);
                }
                Ok(None) | Err(_) => {
                    log::warn!(
                        "[hubble] 후보 등록 실패; seen ledger 미기록 {}: {e}",
                        discovery.title
                    );
                }
            },
        }

        if should_mark_seen {
            seen.insert(
                id.clone(),
                serde_json::json!({
                    "first_seen": now_marker(),
                    "task_id": id,
                    "status": "pending_proposal"
                }),
            );
        }
    }

    if let Err(e) = write_seen(seen_path, &seen) {
        log::warn!(
            "[hubble] seen ledger write failed {}: {e}",
            seen_path.display()
        );
    }

    created
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
    let seen_path = std::env::var("HUBBLE_SEEN_JSON_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_path.join(".canopus/hubble/seen.json"));
    let table = Arc::new(FileTaskTable::new(PathBuf::from(&tasks_path)));
    let interval = Duration::from_secs(3600);

    loop {
        log::info!("[hubble] 스캔 시작: {}", repo_path.display());
        let findings = scanner::scan(&repo_path).await;
        register_discoveries(table.as_ref(), &seen_path, &findings).await;

        log::info!("[hubble] 다음 스캔까지 {}초 대기", interval.as_secs());
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dysonsphere::status::TaskStatus;

    #[tokio::test]
    async fn registers_discovery_once_as_pending_proposal() {
        let root = std::env::temp_dir().join(format!("hubble-seen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let tasks_path = root.join("tasks.json");
        let seen_path = root.join(".canopus/hubble/seen.json");
        let table = FileTaskTable::new(tasks_path.clone());
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

        let id = discovery_id(&discoveries[0]);
        let stored = table.fetch(&id).await.unwrap().unwrap();
        assert_eq!(stored.meta.status, TaskStatus::PendingProposal);
        assert!(seen_path.exists());
        let _ = fs::remove_dir_all(root);
    }
}
