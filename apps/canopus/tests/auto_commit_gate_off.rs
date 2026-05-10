//! AC1: gate-off behavior — without `CANOPUS_ALLOW_LOCAL_COMMIT`, the watch loop
//! must use `FinalizeMode::DryRun` and never produce a new commit.
//!
//! These tests exercise the env-flag branch at `finalize.rs:85-89` via the public
//! `canopus::cli::run` watch interface (with `--once`). `FinalizeMode` and `env_flag`
//! are `pub(crate)` and inaccessible here; we assert observable behavior instead:
//!   - the finalize sidecar contains "finalize mode: DryRun"
//!   - `git log --oneline` shows only the initial commit, never a canopus commit
//!
//! `ENV_LOCK` from `canopus::test_support` is also `pub(crate)`. Each test here
//! acquires its own process-local `ENV_LOCK` (defined below) to prevent races when
//! multiple tests mutate `CANOPUS_ALLOW_LOCAL_COMMIT`. This is safe because
//! integration tests in the same test binary share a process.
#![allow(clippy::await_holding_lock)]

use canopus::cli;
use std::fs;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

/// Process-local env guard for tests that mutate `CANOPUS_ALLOW_LOCAL_COMMIT`.
/// Prevents data races when the test harness runs tests in parallel threads.
static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn git_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-gate-off-{name}-{}", std::process::id()));
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
    // Initial commit so the repo has a HEAD.
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

/// Returns the subject of the latest commit in `repo`.
fn latest_commit_subject(repo: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Builds a minimal `tasks.json` payload that passes the watch-loop filters:
/// `status=Processed`, `approval_state=approved`, `finalize_requested_at` present.
fn approved_tasks_json(agenda_id: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!([{
        "task_id": agenda_id,
        "payload": {
            "agenda_id": agenda_id,
            "approval_state": "approved",
            "finalize_requested_at": "2026-05-09T00:00:00Z"
        },
        "meta": { "status": "Processed" }
    }]))
    .unwrap()
}

fn run_id_for(agenda_id: &str) -> String {
    format!("{agenda_id}-{agenda_id}")
}

async fn run_watch_once(
    repo: &std::path::Path,
    state: &std::path::Path,
    tasks_path: &std::path::Path,
) {
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
}

// ─────────────────────────────────────────────────────────────
// TC1: CANOPUS_ALLOW_LOCAL_COMMIT unset → DryRun, no new commit
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_uses_dry_run_when_env_flag_unset() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var("CANOPUS_ALLOW_LOCAL_COMMIT");

    let repo = git_repo("gate-off-unset");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // Write a changed file so the watch loop has something to inspect.
    fs::write(repo.join("work.txt"), "agent output\n").unwrap();

    let tasks_path = state.join("tasks.json");
    fs::write(&tasks_path, approved_tasks_json("agenda-gate-off-unset")).unwrap();

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

    // Finalize sidecar must exist and declare DryRun mode.
    let finalize_path = state.join("runs").join(format!(
        "{}-finalize.txt",
        run_id_for("agenda-gate-off-unset")
    ));
    assert!(
        finalize_path.exists(),
        "watch must write a finalize sidecar even in DryRun mode"
    );
    let record = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        record.contains("finalize mode: DryRun"),
        "finalize record must advertise DryRun mode; got: {record}"
    );
    assert!(
        record.contains("dry-run: skipped git add/commit/push"),
        "finalize record must confirm skip; got: {record}"
    );

    // No new commit must have been made on the repo.
    let subject = latest_commit_subject(&repo);
    assert_eq!(
        subject, "init",
        "no commit beyond 'init' must exist when CANOPUS_ALLOW_LOCAL_COMMIT is unset; got: {subject}"
    );

    let _ = fs::remove_dir_all(repo);
}

// ─────────────────────────────────────────────────────────────
// TC2: CANOPUS_ALLOW_LOCAL_COMMIT=0 → DryRun (explicit zero is still off)
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_uses_dry_run_when_env_flag_explicitly_zero() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "0");

    let repo = git_repo("gate-off-zero");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    fs::write(repo.join("work.txt"), "agent output\n").unwrap();

    let tasks_path = state.join("tasks.json");
    fs::write(&tasks_path, approved_tasks_json("agenda-gate-off-zero")).unwrap();

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

    let finalize_path = state.join("runs").join(format!(
        "{}-finalize.txt",
        run_id_for("agenda-gate-off-zero")
    ));
    let record = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        record.contains("finalize mode: DryRun"),
        "CANOPUS_ALLOW_LOCAL_COMMIT=0 must still select DryRun; got: {record}"
    );

    let subject = latest_commit_subject(&repo);
    assert_eq!(
        subject, "init",
        "no commit beyond 'init' must exist when CANOPUS_ALLOW_LOCAL_COMMIT=0; got: {subject}"
    );

    std::env::remove_var("CANOPUS_ALLOW_LOCAL_COMMIT");
    let _ = fs::remove_dir_all(repo);
}

// ─────────────────────────────────────────────────────────────
// TC3: CANOPUS_ALLOW_LOCAL_COMMIT=1 → LocalCommitOnly selected
//
// This test verifies that setting the env flag activates LocalCommitOnly mode
// (the finalize record advertises it). Because the pre-flight checks require
// .canopus/ to be gitignored, we set that up so the mode can proceed.
// ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_uses_local_commit_only_when_env_flag_set() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");

    let repo = git_repo("gate-on");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // Pre-flight requirement: .canopus/ must be in .gitignore.
    fs::write(repo.join(".gitignore"), ".canopus/\n").unwrap();
    Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add gitignore"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args([
            "checkout",
            "-b",
            &format!("canopus/{}", run_id_for("agenda-gate-on")),
        ])
        .current_dir(&repo)
        .output()
        .unwrap();

    // Write a tracked change for the commit to pick up.
    fs::write(repo.join("work.txt"), "agent output\n").unwrap();

    let tasks_path = state.join("tasks.json");
    fs::write(&tasks_path, approved_tasks_json("agenda-gate-on")).unwrap();

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

    let finalize_path = state
        .join("runs")
        .join(format!("{}-finalize.txt", run_id_for("agenda-gate-on")));
    assert!(
        finalize_path.exists(),
        "watch must write a finalize sidecar in LocalCommitOnly mode"
    );
    let record = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        record.contains("finalize mode: LocalCommitOnly"),
        "finalize record must advertise LocalCommitOnly mode when CANOPUS_ALLOW_LOCAL_COMMIT=1; got: {record}"
    );
    // LocalCommitOnly must NOT push or create a PR.
    assert!(
        !record.contains("git push"),
        "LocalCommitOnly must not push; got: {record}"
    );

    // A new commit must exist (beyond the initial "init" + ".gitignore" commits).
    let subject = latest_commit_subject(&repo);
    assert!(
        subject.contains("agenda-gate-on") || subject.contains("complete agenda"),
        "LocalCommitOnly must produce a commit whose subject references the agenda; got: {subject}"
    );

    std::env::remove_var("CANOPUS_ALLOW_LOCAL_COMMIT");
    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn watch_gate_on_upgrades_prior_dry_run_sidecar_to_local_commit() {
    let _guard = ENV_LOCK.lock().unwrap();

    let repo = git_repo("gate-on-upgrades-dry-run");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    fs::write(repo.join(".gitignore"), ".canopus/\n").unwrap();
    Command::new("git")
        .args(["add", ".gitignore"])
        .current_dir(&repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add gitignore"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let agenda_id = "agenda-gate-on-upgrade";
    let run_id = run_id_for(agenda_id);
    let branch = format!("canopus/{run_id}");
    Command::new("git")
        .args(["checkout", "-b", &branch])
        .current_dir(&repo)
        .output()
        .unwrap();

    fs::write(repo.join("work.txt"), "agent output\n").unwrap();
    let tasks_path = state.join("tasks.json");
    fs::write(&tasks_path, approved_tasks_json(agenda_id)).unwrap();
    let finalize_path = state.join("runs").join(format!("{run_id}-finalize.txt"));

    std::env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "0");
    run_watch_once(&repo, &state, &tasks_path).await;
    let dry_record = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        dry_record.contains("finalize mode: DryRun"),
        "first watch pass should record DryRun; got: {dry_record}"
    );
    assert_eq!(latest_commit_subject(&repo), "add gitignore");

    std::env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    run_watch_once(&repo, &state, &tasks_path).await;
    let upgraded_record = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        upgraded_record.contains("finalize mode: LocalCommitOnly"),
        "gate-on watch pass must upgrade prior DryRun record; got: {upgraded_record}"
    );
    assert!(
        upgraded_record.contains("commit: "),
        "upgraded record should capture the local commit sha; got: {upgraded_record}"
    );
    assert!(
        latest_commit_subject(&repo).contains("complete agenda"),
        "gate-on watch pass should commit pending work"
    );

    std::env::remove_var("CANOPUS_ALLOW_LOCAL_COMMIT");
    let _ = fs::remove_dir_all(repo);
}
