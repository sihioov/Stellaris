#![allow(clippy::await_holding_lock)]

use canopus::cli;
use canopus::core::helper_artifact_task_id;
use std::fs;
use std::process::Command;
use std::sync::{LazyLock, Mutex, MutexGuard};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

const GUARDED_ENV_VARS: &[&str] = &[
    "CANOPUS_PRE_RUN_HELPERS",
    "CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY",
    "CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES",
    "PATH",
];

struct EnvGuard {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn without_helpers() -> Self {
        let guard = ENV_LOCK.lock().unwrap();
        let saved = save_env();
        std::env::remove_var("CANOPUS_PRE_RUN_HELPERS");
        std::env::remove_var("CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY");
        std::env::remove_var("CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES");
        Self {
            _guard: guard,
            saved,
        }
    }

    fn with_helper(mode: &str) -> Self {
        let guard = ENV_LOCK.lock().unwrap();
        let saved = save_env();
        std::env::set_var("CANOPUS_PRE_RUN_HELPERS", mode);
        std::env::remove_var("CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY");
        std::env::remove_var("CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES");
        Self {
            _guard: guard,
            saved,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn save_env() -> Vec<(&'static str, Option<String>)> {
    GUARDED_ENV_VARS
        .iter()
        .map(|key| (*key, std::env::var(key).ok()))
        .collect()
}

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
    let _env = EnvGuard::without_helpers();
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
    let _env = EnvGuard::without_helpers();
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
    let _env = EnvGuard::without_helpers();
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

#[tokio::test]
async fn submit_accepts_and_stores_upstream_task_provenance() {
    let _env = EnvGuard::without_helpers();
    let repo = git_repo("cli-submit-upstream-provenance");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "UPSTREAM-META-1".to_string(),
        "--task-id".to_string(),
        "discord-task-42".to_string(),
        "--task-status".to_string(),
        "PendingReview".to_string(),
        "--task-created-at".to_string(),
        "2026-05-03T01:02:03Z".to_string(),
        "--task-updated-at".to_string(),
        "2026-05-03T04:05:06Z".to_string(),
        "preserve upstream provenance".to_string(),
    ])
    .await
    .unwrap();

    let provenance = fs::read_to_string(
        state
            .join("artifacts")
            .join("upstream-meta-1-discord-task-42-upstream-provenance")
            .join("runtime-log.md"),
    )
    .unwrap();
    assert!(provenance.contains("source_task_id: discord-task-42"));
    assert!(provenance.contains("task_status: PendingReview"));
    assert!(provenance.contains("task_created_at: 2026-05-03T01:02:03Z"));
    assert!(provenance.contains("task_updated_at: 2026-05-03T04:05:06Z"));

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
        .any(|payload| payload.contains("task_id=discord-task-42")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("Upstream task status: PendingReview")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("Upstream task created_at: 2026-05-03T01:02:03Z")));
    assert!(payloads
        .iter()
        .any(|payload| payload.contains("Upstream task updated_at: 2026-05-03T04:05:06Z")));

    let run_record_path = state
        .join("runs")
        .join("upstream-meta-1-discord-task-42.json");
    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&run_record_path).unwrap()).unwrap();
    assert!(run_records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "upstream-provenance" && record["status"] == "ok"));

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_issue_only_flow_ignores_project_mode_without_project_identity() {
    let _env = EnvGuard::without_helpers();
    let repo = git_repo("cli-submit-issue-only-project-mode");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "ISSUE-ONLY-1".to_string(),
        "--github-issue-number".to_string(),
        "7".to_string(),
        "--github-project-mode".to_string(),
        "dry-run-offline".to_string(),
        "--github-project-status-field-name".to_string(),
        "Status".to_string(),
        "issue-only request".to_string(),
    ])
    .await
    .unwrap();

    let run_record_path = state.join("runs").join("issue-only-1.json");
    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&run_record_path).unwrap()).unwrap();
    let stage_names = run_records
        .as_array()
        .unwrap()
        .iter()
        .map(|record| record["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert!(stage_names.contains(&"complete".to_string()));
    assert!(!stage_names.contains(&"github-project".to_string()));
    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_project_validate_mode_fails_gate_before_credentials() {
    let _env = EnvGuard::without_helpers();
    let repo = git_repo("cli-submit-project-gate-before-credentials");
    let state = repo.join(".canopus");

    let err = cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "PROJECT-GATE-1".to_string(),
        "--github-project-id".to_string(),
        "PVT_project".to_string(),
        "--github-project-item-id".to_string(),
        "PVTI_existing".to_string(),
        "--github-project-status-field-name".to_string(),
        "Status".to_string(),
        "--github-project-status-option-name".to_string(),
        "Ready".to_string(),
        "--github-project-mode".to_string(),
        "validate-read-only".to_string(),
        "project validate request".to_string(),
    ])
    .await
    .unwrap_err();

    assert!(err.to_string().contains("CANOPUS_ENABLE_GITHUB=1"));
    assert!(!err.to_string().contains("GITHUB_TOKEN"));
    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_enabled_mock_helper_persists_provenance_and_attaches_context() {
    let _env = EnvGuard::with_helper("mock");

    let repo = git_repo("cli-submit-helper-mock");
    let state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "HELPER-MOCK-1".to_string(),
        "use helper context".to_string(),
    ])
    .await
    .unwrap();

    let plan_helper_id = helper_artifact_task_id("helper-mock-1-TASK-1-plan", "mock-context", 0);
    let helper_path = state
        .join("artifacts")
        .join(plan_helper_id)
        .join("helper-provenance.md");
    let provenance = fs::read_to_string(&helper_path).unwrap();
    assert!(provenance.contains("helper: mock-context"));
    assert!(provenance.contains("role: planner"));
    assert!(provenance.contains("backend_identity: mock-pre-run-helper"));
    assert!(provenance.contains("status: ok"));
    assert!(provenance.contains("attached_to: prior_artifacts before helper-mock-1-TASK-1-plan"));
    assert!(provenance.contains("read_only_check: passed"));

    let coder_output = fs::read_to_string(repo.join("canopus-mock-output.txt")).unwrap();
    assert!(coder_output.contains("helper-provenance.md"));
    assert!(coder_output.contains("Mock pre-run helper context"));

    let run_records: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state.join("runs/helper-mock-1.json")).unwrap())
            .unwrap();
    assert!(run_records
        .as_array()
        .unwrap()
        .iter()
        .any(|record| record["name"] == "helper:plan" && record["status"] == "ok"));

    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn submit_repo_explore_helper_detects_ignored_path_mutation_and_continues() {
    use std::os::unix::fs::PermissionsExt;

    let _env = EnvGuard::with_helper("repo-explore");

    let repo = git_repo("cli-submit-helper-mutation");
    let state = repo.join(".canopus");
    let fake_bin = std::env::temp_dir().join(format!(
        "canopus-fake-bin-helper-mutation-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&fake_bin);
    fs::create_dir_all(&fake_bin).unwrap();
    let fake_omx = fake_bin.join("omx");
    fs::write(
        &fake_omx,
        "#!/bin/sh\nmkdir -p .omx\nprintf mutation > .omx/helper-mutated.txt\nprintf helper-output\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&fake_omx).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake_omx, perms).unwrap();
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old_path}", fake_bin.display()));

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        state.display().to_string(),
        "--agenda-id".to_string(),
        "HELPER-MUTATION-1".to_string(),
        "detect helper mutation".to_string(),
    ])
    .await
    .unwrap();

    let plan_helper_id =
        helper_artifact_task_id("helper-mutation-1-TASK-1-plan", "repo-explore", 0);
    let provenance = fs::read_to_string(
        state
            .join("artifacts")
            .join(plan_helper_id)
            .join("helper-provenance.md"),
    )
    .unwrap();
    assert!(provenance.contains("helper: repo-explore"));
    assert!(provenance.contains("status: failed"));
    assert!(provenance.contains("read-only guard failed"));
    assert!(repo.join("canopus-mock-output.txt").exists());

    let _ = fs::remove_dir_all(repo);
    let _ = fs::remove_dir_all(fake_bin);
}
