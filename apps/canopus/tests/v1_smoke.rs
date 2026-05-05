use canopus::cli;
use chrono::Utc;
use dysonsphere::{
    db::{FileTaskTable, TaskTable, TransitionOutcome},
    message::{TaskMessage, TaskMeta, TaskType},
    status::TaskStatus,
};
use laniakea::worker::run_file_loop;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::sync::Mutex;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

struct EnvRestore {
    values: Vec<(&'static str, Option<String>)>,
}

impl EnvRestore {
    fn capture(keys: &[&'static str]) -> Self {
        Self {
            values: keys.iter().map(|key| (*key, env::var(key).ok())).collect(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
        }
    }
}

fn git_repo(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "canopus@example.invalid"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Canopus Test"])
        .current_dir(&root)
        .output()
        .unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .unwrap();

    root
}

fn pending_canopus_task() -> TaskMessage {
    TaskMessage {
        task_id: "V1-SMOKE-1".to_string(),
        task_type: TaskType::Custom("canopus.agent".to_string()),
        payload: serde_json::json!({
            "request": "v1 smoke request",
            "agenda_id": "agenda-v1-smoke-1"
        })
        .to_string(),
        meta: TaskMeta {
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    }
}

async fn ton618_bounded_dispatch(table: &FileTaskTable) {
    let pending = table.fetch_pending().await.unwrap();
    assert_eq!(pending.len(), 1, "smoke must start with one pending task");
    for task in pending {
        assert_eq!(
            table
                .transition(&task.task_id, TaskStatus::Pending, TaskStatus::Dispatched)
                .await
                .unwrap(),
            TransitionOutcome::Applied
        );
    }
}

fn install_fake_canopus(bin_dir: &Path) -> PathBuf {
    fs::create_dir_all(bin_dir).unwrap();
    let args_file = bin_dir.join("canopus-args.txt");

    #[cfg(unix)]
    {
        let fake = bin_dir.join("canopus");
        fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
                args_file.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake, permissions).unwrap();
    }

    #[cfg(windows)]
    {
        let fake = bin_dir.join("canopus.bat");
        fs::write(
            &fake,
            format!(
                "@echo off\r\necho %* > \"{}\"\r\nexit /b 0\r\n",
                args_file.display()
            ),
        )
        .unwrap();
    }

    args_file
}

#[tokio::test]
async fn v1_smoke_closes_pending_task_to_finalize_record_offline() {
    let _env_guard = ENV_LOCK.lock().await;
    let _restore = EnvRestore::capture(&[
        "PATH",
        "CANOPUS_REPO_PATH",
        "CANOPUS_STATE_PATH",
        "CANOPUS_TIMEOUT_SECS",
        "CANOPUS_ENABLE_GITHUB",
        "CANOPUS_ENABLE_LIVE_MUTATIONS",
        "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION",
        "DISCORD_WEBHOOK_URL",
    ]);

    let repo = git_repo("v1-smoke");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks_path = state.join("tasks.json");
    let table = Arc::new(FileTaskTable::new(tasks_path.clone()));
    table.create(pending_canopus_task()).await.unwrap();

    ton618_bounded_dispatch(&table).await;
    assert_eq!(
        table
            .fetch("V1-SMOKE-1")
            .await
            .unwrap()
            .unwrap()
            .meta
            .status,
        TaskStatus::Dispatched
    );

    let bin_dir = repo.join("bin");
    let args_file = install_fake_canopus(&bin_dir);
    let original_path = env::var("PATH").unwrap_or_default();
    env::set_var("PATH", format!("{}:{original_path}", bin_dir.display()));
    env::set_var("CANOPUS_REPO_PATH", &repo);
    env::set_var("CANOPUS_STATE_PATH", &state);
    env::set_var("CANOPUS_TIMEOUT_SECS", "2");
    env::set_var("CANOPUS_ENABLE_GITHUB", "0");
    env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "0");
    env::set_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION", "0");
    env::remove_var("DISCORD_WEBHOOK_URL");

    let worker = tokio::time::timeout(
        Duration::from_millis(300),
        run_file_loop(Arc::clone(&table), Duration::from_millis(50)),
    )
    .await;
    assert!(
        worker.is_err(),
        "laniakea file loop is expected to keep running"
    );
    assert!(
        args_file.exists(),
        "fake canopus must be invoked by laniakea"
    );
    let canopus_args = fs::read_to_string(&args_file).unwrap();
    assert!(canopus_args.contains("submit"));
    assert!(canopus_args.contains("--task-id\nV1-SMOKE-1"));

    assert_eq!(
        table
            .fetch("V1-SMOKE-1")
            .await
            .unwrap()
            .unwrap()
            .meta
            .status,
        TaskStatus::PendingReview
    );

    assert_eq!(
        table
            .transition(
                "V1-SMOKE-1",
                TaskStatus::PendingReview,
                TaskStatus::Processed
            )
            .await
            .unwrap(),
        TransitionOutcome::Applied
    );
    let mut stored_tasks =
        serde_json::from_str::<Vec<serde_json::Value>>(&fs::read_to_string(&tasks_path).unwrap())
            .unwrap();
    let task = stored_tasks
        .iter_mut()
        .find(|task| task["task_id"] == "V1-SMOKE-1")
        .unwrap();
    task["payload"] = serde_json::Value::String(
        serde_json::json!({
            "request": "v1 smoke request",
            "agenda_id": "agenda-v1-smoke-1",
            "approval_state": "approved",
            "finalize_requested_at": "2026-05-05T00:00:00Z"
        })
        .to_string(),
    );
    fs::write(
        &tasks_path,
        serde_json::to_string_pretty(&stored_tasks).unwrap(),
    )
    .unwrap();

    cli::run(vec![
        "canopus".to_string(),
        "watch".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--once".to_string(),
        tasks_path.display().to_string(),
    ])
    .await
    .unwrap();

    let finalize_path = state.join("runs").join("agenda-v1-smoke-1-finalize.txt");
    assert!(
        finalize_path.exists(),
        "watch must create {}",
        finalize_path.display()
    );
    let finalize_record = fs::read_to_string(&finalize_path).unwrap();
    assert!(finalize_record.contains("finalize mode: DryRun"));
    assert!(finalize_record.contains("agenda_id: agenda-v1-smoke-1"));
    assert!(finalize_record.contains("dry-run: skipped git add/commit/push"));
    let delivery_gate_path = state
        .join("runs")
        .join("agenda-v1-smoke-1-delivery-gate.json");
    assert!(
        delivery_gate_path.exists(),
        "watch must create {}",
        delivery_gate_path.display()
    );
    let delivery_gate: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&delivery_gate_path).unwrap()).unwrap();
    assert_eq!(delivery_gate["status"], "Denied");
    assert_eq!(delivery_gate["discord_approved"], true);
    assert_eq!(delivery_gate["pr_gate_enabled"], false);

    let _ = fs::remove_dir_all(repo);
}
