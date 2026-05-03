use canopus::cli;
use chrono::Utc;
use dysonsphere::{
    message::{TaskMessage, TaskMeta, TaskType},
    status::TaskStatus,
};
use std::fs;
use std::process::Command;

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
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

#[tokio::test]
async fn watch_and_explicit_finalize_share_idempotent_dry_run_record() {
    let repo = git_repo("p0-local-dry-run-loop");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    let task = TaskMessage {
        task_id: "AGENDA-P0-1".to_string(),
        task_type: TaskType::Custom("canopus.agent".to_string()),
        payload: "agenda_id=agenda-p0-1\nrole=reviewer\n".to_string(),
        meta: TaskMeta {
            status: TaskStatus::Processed,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    };
    let tasks_path = state.join("tasks.json");
    fs::write(
        &tasks_path,
        serde_json::to_string_pretty(&vec![task]).unwrap(),
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

    let finalize_path = state.join("runs").join("agenda-p0-1-finalize.txt");
    assert!(
        finalize_path.exists(),
        "watch must create {}",
        finalize_path.display()
    );
    let watch_record = fs::read_to_string(&finalize_path).unwrap();
    assert!(watch_record.contains("finalize mode: DryRun"));
    assert!(watch_record.contains("agenda_id: agenda-p0-1"));
    assert!(watch_record.contains("dry-run: skipped git add/commit/push"));

    cli::run(vec![
        "canopus".to_string(),
        "finalize".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "AGENDA-P0-1".to_string(),
    ])
    .await
    .unwrap();
    let explicit_record = fs::read_to_string(&finalize_path).unwrap();
    assert_eq!(
        watch_record, explicit_record,
        "explicit finalize should resolve and write the same dry-run finalize path"
    );

    let modified_before_second_watch = fs::metadata(&finalize_path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));

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
    let second_watch_record = fs::read_to_string(&finalize_path).unwrap();
    let modified_after_second_watch = fs::metadata(&finalize_path).unwrap().modified().unwrap();
    assert_eq!(
        explicit_record, second_watch_record,
        "second watch tick must preserve an existing finalize record"
    );
    assert_eq!(
        modified_before_second_watch, modified_after_second_watch,
        "second watch tick must not rewrite an existing finalize record"
    );

    let _ = fs::remove_dir_all(repo);
}
