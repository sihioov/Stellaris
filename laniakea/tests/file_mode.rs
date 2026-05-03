use dysonsphere::{
    db::{task_table_file::FileTaskTable, TaskTable},
    message::{TaskMessage, TaskMeta, TaskType},
    status::TaskStatus,
};
use laniakea::worker::run_file_loop;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::{env, fs};
use tokio::sync::Mutex;

fn make_dispatched_task(id: &str, task_type: TaskType) -> TaskMessage {
    TaskMessage {
        task_id: id.to_string(),
        task_type,
        payload: format!("payload-{id}"),
        meta: TaskMeta {
            status: TaskStatus::Dispatched,
            ..TaskMeta::default()
        },
    }
}

fn make_pending_proposal_task(id: &str) -> TaskMessage {
    TaskMessage {
        task_id: id.to_string(),
        task_type: TaskType::Bug,
        payload: format!("payload-{id}"),
        meta: TaskMeta {
            status: TaskStatus::PendingProposal,
            ..TaskMeta::default()
        },
    }
}

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct EnvRestore {
    path: Option<String>,
    repo: Option<String>,
    state: Option<String>,
    timeout: Option<String>,
}

impl EnvRestore {
    fn capture() -> Self {
        Self {
            path: env::var("PATH").ok(),
            repo: env::var("CANOPUS_REPO_PATH").ok(),
            state: env::var("CANOPUS_STATE_PATH").ok(),
            timeout: env::var("CANOPUS_TIMEOUT_SECS").ok(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        restore_env("PATH", self.path.as_deref());
        restore_env("CANOPUS_REPO_PATH", self.repo.as_deref());
        restore_env("CANOPUS_STATE_PATH", self.state.as_deref());
        restore_env("CANOPUS_TIMEOUT_SECS", self.timeout.as_deref());
    }
}

fn restore_env(key: &str, value: Option<&str>) {
    if let Some(value) = value {
        env::set_var(key, value);
    } else {
        env::remove_var(key);
    }
}

#[tokio::test]
async fn file_mode_processes_dispatched_tasks_to_pending_review() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let tasks = vec![
        make_dispatched_task("T1", TaskType::NewsA),
        make_dispatched_task("T2", TaskType::NewsA),
    ];
    let json = serde_json::to_string(&tasks).unwrap();
    fs::write(file.path(), &json).unwrap();

    let table = Arc::new(FileTaskTable::new(file.path().to_path_buf()));

    let result = tokio::time::timeout(
        Duration::from_secs(3),
        run_file_loop(Arc::clone(&table), Duration::from_millis(100)),
    )
    .await;

    // 루프는 무한이므로 timeout Err가 정상
    assert!(result.is_err(), "expected timeout, worker exited early");

    let t1 = table.fetch("T1").await.unwrap().unwrap();
    let t2 = table.fetch("T2").await.unwrap().unwrap();
    assert_eq!(
        t1.meta.status,
        TaskStatus::PendingReview,
        "T1 should be PendingReview"
    );
    assert_eq!(
        t2.meta.status,
        TaskStatus::PendingReview,
        "T2 should be PendingReview"
    );

    // idempotency: PendingReview는 재처리되지 않음
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t1_again = table.fetch("T1").await.unwrap().unwrap();
    assert_eq!(
        t1_again.meta.status,
        TaskStatus::PendingReview,
        "T1 must not be re-dispatched"
    );
}

#[tokio::test]
async fn file_mode_does_not_process_pending_proposals() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let tasks = vec![make_pending_proposal_task("P1")];
    let json = serde_json::to_string(&tasks).unwrap();
    fs::write(file.path(), &json).unwrap();

    let table = Arc::new(FileTaskTable::new(file.path().to_path_buf()));

    let result = tokio::time::timeout(
        Duration::from_millis(250),
        run_file_loop(Arc::clone(&table), Duration::from_millis(50)),
    )
    .await;

    assert!(result.is_err(), "expected timeout, worker exited early");
    let task = table.fetch("P1").await.unwrap().unwrap();
    assert_eq!(task.meta.status, TaskStatus::PendingProposal);
}

#[tokio::test]
async fn file_mode_preserves_concurrent_status_change_after_canopus_success() {
    let _env_guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    let _restore = EnvRestore::capture();
    let temp = tempfile::tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let fake_canopus = bin_dir.join("canopus");
    let args_file = temp.path().join("canopus-args.txt");
    let started_file = temp.path().join("canopus-started");
    let continue_file = temp.path().join("canopus-continue");
    fs::write(
        &fake_canopus,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ntouch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.05; done\nexit 0\n",
            args_file.display(),
            started_file.display(),
            continue_file.display()
        ),
    )
    .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&fake_canopus).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_canopus, permissions).unwrap();
    }

    let original_path = env::var("PATH").unwrap_or_default();
    env::set_var("PATH", format!("{}:{original_path}", bin_dir.display()));
    env::set_var("CANOPUS_REPO_PATH", temp.path().join("repo"));
    env::set_var("CANOPUS_STATE_PATH", temp.path().join(".canopus"));
    env::set_var("CANOPUS_TIMEOUT_SECS", "2");

    let file = tempfile::NamedTempFile::new().unwrap();
    let tasks = vec![make_dispatched_task(
        "C1",
        TaskType::Custom("canopus.agent".to_string()),
    )];
    fs::write(file.path(), serde_json::to_string(&tasks).unwrap()).unwrap();
    let table = Arc::new(FileTaskTable::new(file.path().to_path_buf()));

    let worker = tokio::spawn(run_file_loop(Arc::clone(&table), Duration::from_millis(25)));

    wait_for_path(&started_file).await;
    table
        .transition("C1", TaskStatus::Dispatched, TaskStatus::Failed)
        .await
        .unwrap();
    fs::write(&continue_file, "").unwrap();
    wait_for_path(&args_file).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    worker.abort();

    let task = table.fetch("C1").await.unwrap().unwrap();
    assert_eq!(
        task.meta.status,
        TaskStatus::Failed,
        "worker must not resurrect a task whose status changed during canopus execution"
    );

    let args = fs::read_to_string(args_file).unwrap();
    assert!(args.contains("--task-status\nDispatched"));
    assert!(args.contains("--task-type\ncustom:canopus.agent"));
}

async fn wait_for_path(path: &std::path::Path) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {}", path.display()));
}
