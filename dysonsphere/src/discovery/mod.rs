use crate::db::TaskTable;
use crate::error::Result;
use crate::message::TaskMessage;
use chrono::{SecondsFormat, Utc};
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

/// A discovery is a candidate finding produced by a collector/scanner service.
///
/// Discoveries are not executable work by themselves. They are converted into
/// `TaskMessage` records with `PendingProposal` status and must pass through
/// the human approval gate before TON618 can dispatch them.
pub trait Discovery {
    fn id(&self) -> String;
    fn title(&self) -> &str;
    fn to_task_message(&self, id: &str) -> TaskMessage;
}

pub fn discovery_id_fnv1a(prefix: &str, parts: &[&[u8]]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for part in parts {
        stable_hash_part(&mut hash, part);
    }
    format!("{prefix}-{hash:016x}")
}

pub fn now_marker_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub async fn register_discoveries<T, D>(table: &T, seen_path: &Path, discoveries: &[D]) -> usize
where
    T: TaskTable + Sync,
    D: Discovery,
{
    let mut seen = read_seen(seen_path);
    let mut created = 0usize;

    for discovery in discoveries {
        let id = discovery.id();
        if seen.contains_key(&id) {
            log::info!("[discovery] already seen: {}", discovery.title());
            continue;
        }

        let task = discovery.to_task_message(&id);
        let mut should_mark_seen = false;
        match table.create(task).await {
            Ok(()) => {
                created += 1;
                should_mark_seen = true;
                log::info!("[discovery] registered proposal: {}", discovery.title());
                notify_discord(&format!(
                    "📝 새 후보 발견: {} — `!propose-approve {}`",
                    discovery.title(),
                    id
                ));
            }
            Err(err) => match table.fetch(&id).await {
                Ok(Some(_)) => {
                    should_mark_seen = true;
                    log::info!(
                        "[discovery] restored seen ledger for already-registered proposal: {}",
                        discovery.title()
                    );
                }
                Ok(None) | Err(_) => {
                    log::warn!(
                        "[discovery] proposal registration failed; seen ledger not updated for {}: {err}",
                        discovery.title()
                    );
                }
            },
        }

        if should_mark_seen {
            seen.insert(
                id.clone(),
                serde_json::json!({
                    "first_seen": now_marker_rfc3339(),
                    "task_id": id,
                    "status": "pending_proposal"
                }),
            );
        }
    }

    if let Err(err) = write_seen(seen_path, &seen) {
        log::warn!(
            "[discovery] seen ledger write failed {}: {err}",
            seen_path.display()
        );
    }

    created
}

pub fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        let _ = ureq::post(&url).send_json(body);
    }
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

fn read_seen(path: &Path) -> Map<String, Value> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<Map<String, Value>>(&raw).unwrap_or_default(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(err) => {
            log::warn!(
                "[discovery] seen ledger read failed {}: {err}",
                path.display()
            );
            Map::new()
        }
    }
}

fn write_seen(path: &Path, seen: &Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(seen)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{FileTaskTable, TaskTable};
    use crate::message::{TaskMeta, TaskType};
    use crate::status::TaskStatus;

    #[derive(Clone)]
    struct TestDiscovery {
        title: String,
        description: String,
    }

    impl Discovery for TestDiscovery {
        fn id(&self) -> String {
            discovery_id_fnv1a(
                "test",
                &[self.title.as_bytes(), self.description.as_bytes()],
            )
        }

        fn title(&self) -> &str {
            &self.title
        }

        fn to_task_message(&self, id: &str) -> TaskMessage {
            TaskMessage {
                task_id: id.to_string(),
                task_type: TaskType::Bug,
                payload: format!("{}: {}", self.title, self.description),
                meta: TaskMeta {
                    status: TaskStatus::PendingProposal,
                    ..TaskMeta::default()
                },
            }
        }
    }

    #[test]
    fn fnv_id_is_stable_and_prefixed() {
        let id = discovery_id_fnv1a("kepler", &[b"Bug", b"title", b"description"]);
        assert_eq!(id, "kepler-cfb118a0768a5d22");
    }

    #[tokio::test]
    async fn register_discoveries_once_as_pending_proposal_with_rfc3339_seen() {
        let root = std::env::temp_dir().join(format!("discovery-seen-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let tasks_path = root.join("tasks.json");
        let seen_path = root.join(".canopus/discovery/seen.json");
        let table = FileTaskTable::new(tasks_path);
        let discoveries = vec![TestDiscovery {
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
        let seen = read_seen(&seen_path);
        let first_seen = seen[&id]["first_seen"].as_str().unwrap();
        assert!(!first_seen.starts_with("unix:"));
        chrono::DateTime::parse_from_rfc3339(first_seen).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
