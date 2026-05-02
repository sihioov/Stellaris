use canopus::cli;
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
async fn submit_creates_branch_patch_backend_task_and_artifacts() {
    let repo = git_repo("cli-submit");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "CANOPUS-1".to_string(),
        "add test coverage".to_string(),
    ])
    .await
    .unwrap();

    assert!(repo.join("canopus-mock-output.txt").exists());
    assert!(state
        .join("artifacts")
        .join("canopus-1-TASK-1-plan")
        .join("plan.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("canopus-1-TASK-2-code")
        .join("runtime-log.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("canopus-1-TASK-2-code")
        .join("diff.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("canopus-1-TASK-2-code")
        .join("test-result.md")
        .exists());
    assert!(state
        .join("artifacts")
        .join("canopus-1-TASK-3-review")
        .join("review.md")
        .exists());
    assert!(state.join("tasks.json").exists());
    let run_record_path = state.join("runs").join("canopus-1.json");
    assert!(run_record_path.exists());
    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&run_record_path).unwrap()).unwrap();
    let records = run_records.as_array().unwrap();
    assert!(records.iter().any(|record| record["name"] == "plan"));
    assert!(records.iter().any(|record| record["name"] == "check"));
    assert!(records.iter().any(|record| record["name"] == "complete"));
    for record in records {
        let started_at = record["started_at"].as_str().unwrap();
        let ended_at = record["ended_at"].as_str().unwrap();
        assert!(
            !started_at.starts_with("unix:"),
            "started_at must be RFC3339, got {started_at}"
        );
        chrono::DateTime::parse_from_rfc3339(started_at).unwrap();
        chrono::DateTime::parse_from_rfc3339(ended_at).unwrap();
    }

    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&branch.stdout).trim(),
        "canopus/canopus-1"
    );

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_records_failed_prepare_stage() {
    let repo = git_repo("cli-submit-failed-prepare");
    let state = repo.join(".canopus");
    fs::write(repo.join("dirty.txt"), "dirty\n").unwrap();

    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "CANOPUS-1".to_string(),
        "add test coverage".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("worktree is not clean"));
    let run_record_path = state.join("runs").join("canopus-1.json");
    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&run_record_path).unwrap()).unwrap();
    let records = run_records.as_array().unwrap();
    assert!(records
        .iter()
        .any(|record| record["name"] == "prepare" && record["status"] == "failed"));
    let failed = records
        .iter()
        .find(|record| record["name"] == "prepare" && record["status"] == "failed")
        .unwrap();
    let started_at = failed["started_at"].as_str().unwrap();
    assert!(
        !started_at.starts_with("unix:"),
        "failed stage timestamp must be RFC3339"
    );
    chrono::DateTime::parse_from_rfc3339(started_at).unwrap();

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_routes_roles_from_upstream_task_type_and_id() {
    let repo = git_repo("cli-submit-bug-pipeline");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "UPSTREAM-BUG-1".to_string(),
        "--task-type".to_string(),
        "bug".to_string(),
        "fix routed bug".to_string(),
    ])
    .await
    .unwrap();

    let tasks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state.join("tasks.json")).unwrap()).unwrap();
    let payloads = tasks
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["payload"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert!(payloads
        .iter()
        .any(|payload| payload.contains("agenda_id=upstream-bug-1")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("role=analyzer")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("role=coder")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("role=tester")));

    let run_record_path = state.join("runs").join("upstream-bug-1.json");
    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&run_record_path).unwrap()).unwrap();
    let stage_names = run_records
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(stage_names.contains(&"analyzer".to_string()));
    assert!(stage_names.contains(&"code".to_string()));
    assert!(stage_names.contains(&"tester".to_string()));
    assert!(stage_names.contains(&"check".to_string()));
    assert!(stage_names.contains(&"complete".to_string()));

    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&branch.stdout).trim(),
        "canopus/upstream-bug-1"
    );

    let _ = fs::remove_dir_all(repo);
}
