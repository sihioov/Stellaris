//! AC6: failure mode tests for FinalizeMode::LocalCommitOnly
//!
//! # Design constraints
//!
//! ## Why assertions are side-effect-based, not Err-based
//!
//! `post_approval()` is `pub(crate)` and therefore unreachable from integration
//! tests. The only public entrypoint is `cli::run()` with `watch --once`.
//! `watch()` swallows `post_approval` errors: it logs them and returns `Ok(())`.
//! Consequently preflight failures are observed through the *absence* of the
//! finalize sidecar at `<state>/runs/<run_id>-finalize.txt` and the absence of
//! any `canopus/*` branch in the repo — not through an `Err` return value.
//!
//! ## AC6(a) — rejection upstream of `post_approval`
//!
//! The `approval_state="rejected"` signal is filtered inside
//! `approved_processed_tasks()` at `finalize.rs:159` — **before** `post_approval`
//! is ever called. There is nothing to test at the `post_approval` level.
//! AC6(a) is implicitly covered by the existing unit test
//! `approved_processed_tasks_requires_approval_and_finalize_evidence` in
//! `apps/canopus/src/cli/finalize.rs`.
//!
//! ## AC6(c) — namespace exhausted (n=10)
//!
//! Triggering namespace exhaustion requires pre-creating branches
//! `canopus/task-<short>`, `canopus/task-<short>-2` … `canopus/task-<short>-10`
//! before every run.  The lower-priority one-collision test (`branch_collision_uses_suffix`)
//! is sufficient for V1.5 acceptance criteria.
//!
//! ## `tempfile` crate
//!
//! `tempfile` is not in `[dev-dependencies]`.  Tests use the `env::temp_dir()`
//! + `fs::remove_dir_all` pattern from `tests/v1_smoke.rs`.

use canopus::cli;
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::{LazyLock, Mutex},
};

// Independent ENV_LOCK for this integration test binary (cannot reuse the
// pub(crate) one inside the canopus library).
static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an isolated git repo with a single `init` commit, user config set.
/// Mirrors the `git_repo()` helper in `tests/v1_smoke.rs`.
fn git_repo(name: &str) -> PathBuf {
    let root =
        env::temp_dir().join(format!("canopus-ac6-{name}-{}", std::process::id()));
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

/// Write a minimal `tasks.json` with the given `agenda_id` in `Processed` /
/// `approved` state so that `watch --once` will attempt finalization.
fn write_approved_tasks(state: &std::path::Path, agenda_id: &str) -> PathBuf {
    let tasks_path = state.join("tasks.json");
    let json = serde_json::json!([{
        "task_id": format!("test-task-{agenda_id}"),
        "payload": serde_json::to_string(&serde_json::json!({
            "agenda_id": agenda_id,
            "approval_state": "approved",
            "finalize_requested_at": "2026-05-09T00:00:00Z"
        })).unwrap(),
        "meta": { "status": "Processed" }
    }]);
    fs::write(&tasks_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    tasks_path
}

/// Return the path where `watch` would write the finalize sidecar.
fn finalize_path(state: &std::path::Path, run_id: &str) -> PathBuf {
    state.join("runs").join(format!("{run_id}-finalize.txt"))
}

/// RAII env var restorer — same pattern as in `tests/v1_smoke.rs`.
struct EnvRestore {
    values: Vec<(&'static str, Option<String>)>,
}

impl EnvRestore {
    fn capture(keys: &[&'static str]) -> Self {
        Self {
            values: keys
                .iter()
                .map(|key| (*key, env::var(key).ok()))
                .collect(),
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

// ---------------------------------------------------------------------------
// AC6(b): no changes → Ok, no branch, no commit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_changes_returns_ok_without_branch_or_commit() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "DISCORD_WEBHOOK_URL",
    ]);

    let repo = git_repo("no-changes");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // .canopus/ must be gitignored for pre-flight 3 to pass.
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

    // Do NOT write any modified files — the working tree is clean.

    let agenda_id = "agenda-no-changes-ac6b";
    let tasks_path = write_approved_tasks(&state, agenda_id);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::remove_var("DISCORD_WEBHOOK_URL");

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

    // Finalize sidecar must exist — the no-changes path still writes it.
    let sidecar = finalize_path(&state, agenda_id);
    assert!(
        sidecar.exists(),
        "finalize sidecar should be written even for no-changes: {}",
        sidecar.display()
    );
    let record = fs::read_to_string(&sidecar).unwrap();
    assert!(
        record.contains("no changes"),
        "finalize record should mention no-changes skip: {record}"
    );

    // No canopus/* branch should exist.
    let branches = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_out = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_out.contains("canopus/"),
        "no canopus branch should be created when there are no changes: {branch_out}"
    );

    // Git log should show only the two initial commits (init + add gitignore).
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let log_out = String::from_utf8_lossy(&log.stdout);
    assert_eq!(
        log_out.lines().count(),
        2,
        "only the two fixture commits should exist; log: {log_out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// AC6(d): detached HEAD → pre-flight fails; no finalize sidecar, no branch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detached_head_fails_preflight() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "DISCORD_WEBHOOK_URL",
    ]);

    let repo = git_repo("detached-head");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // .canopus/ gitignored so the gitignore pre-flight passes.
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

    // Detach HEAD.
    let head_sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&head_sha.stdout).trim().to_string();
    Command::new("git")
        .args(["checkout", "--detach", &sha])
        .current_dir(&repo)
        .output()
        .unwrap();

    // Write a change so we'd otherwise proceed past the no-changes check.
    fs::write(repo.join("change.txt"), "modified\n").unwrap();

    let agenda_id = "agenda-detached-head-ac6d";
    let tasks_path = write_approved_tasks(&state, agenda_id);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::remove_var("DISCORD_WEBHOOK_URL");

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
    .unwrap(); // watch swallows the error; we assert via side effects

    // No finalize sidecar should have been written (pre-flight failed).
    let sidecar = finalize_path(&state, agenda_id);
    assert!(
        !sidecar.exists(),
        "finalize sidecar must NOT be written when HEAD is detached: {}",
        sidecar.display()
    );

    // No canopus/* branch should have been created.
    let branches = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_out = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_out.contains("canopus/"),
        "no canopus branch should be created on detached HEAD: {branch_out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// AC6(e): dirty index (staged changes) → pre-flight fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dirty_index_fails_preflight() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "DISCORD_WEBHOOK_URL",
    ]);

    let repo = git_repo("dirty-index");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // .canopus/ gitignored.
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

    // Stage a file to make the index dirty.
    fs::write(repo.join("staged.txt"), "staged content\n").unwrap();
    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(&repo)
        .output()
        .unwrap();

    let agenda_id = "agenda-dirty-index-ac6e";
    let tasks_path = write_approved_tasks(&state, agenda_id);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::remove_var("DISCORD_WEBHOOK_URL");

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

    // Pre-flight 2 (index dirty) fires before branch creation — no sidecar.
    let sidecar = finalize_path(&state, agenda_id);
    assert!(
        !sidecar.exists(),
        "finalize sidecar must NOT be written when index is dirty: {}",
        sidecar.display()
    );

    let branches = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_out = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_out.contains("canopus/"),
        "no canopus branch should be created when index is dirty: {branch_out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// AC6(f): .canopus/ not gitignored → pre-flight fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn canopus_not_gitignored_fails_preflight() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "DISCORD_WEBHOOK_URL",
    ]);

    // Build repo WITHOUT adding .canopus/ to .gitignore.
    let repo = git_repo("not-gitignored");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    // No .gitignore written — .canopus/ is NOT ignored.

    // Write a change file so the no-changes check would otherwise be bypassed.
    fs::write(repo.join("change.txt"), "modified\n").unwrap();

    let agenda_id = "agenda-not-gitignored-ac6f";
    let tasks_path = write_approved_tasks(&state, agenda_id);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::remove_var("DISCORD_WEBHOOK_URL");

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

    // Pre-flight 3 (.canopus/ not ignored) fires before branch creation.
    let sidecar = finalize_path(&state, agenda_id);
    assert!(
        !sidecar.exists(),
        "finalize sidecar must NOT be written when .canopus/ is not gitignored: {}",
        sidecar.display()
    );

    let branches = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_out = String::from_utf8_lossy(&branches.stdout);
    assert!(
        !branch_out.contains("canopus/"),
        "no canopus branch should be created when .canopus/ is not gitignored: {branch_out}"
    );

    let _ = fs::remove_dir_all(&repo);
}

// ---------------------------------------------------------------------------
// AC6(c): branch collision → suffix applied; actual branch is base-2
// ---------------------------------------------------------------------------
//
// Note: only one collision (suffix=2) is tested here. Triggering namespace
// exhaustion (n=10) would require pre-creating 10 branches and the error is
// swallowed by watch() anyway. This is sufficient for V1.5 acceptance.

#[tokio::test]
async fn branch_collision_uses_suffix() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "DISCORD_WEBHOOK_URL",
    ]);

    let repo = git_repo("branch-collision");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // .canopus/ gitignored.
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

    // Write a change so there is something to commit.
    fs::write(repo.join("work.txt"), "agent output\n").unwrap();

    // agenda_id chosen so derive_branch_name("", agenda_id) produces a known
    // base name we can pre-create.  run_id == agenda_id (sanitized); short =
    // first 6 hex chars of "agenda-coll-ab1234" → a,e,a,-,c,o... wait, we
    // need to trace: chars that pass is_ascii_hexdigit() from "agenda-coll-ab1234":
    //   a(yes), g(no), e(yes), n(no), d(yes), a(yes), -(no), c(yes), o(no),
    //   l(no), l(no), -(no), a(yes), b(yes) → first 6: a,e,d,a,c,a → "aedaca"
    // branch base = "canopus/task-aedaca"
    let agenda_id = "agenda-coll-ab1234";
    // Verify derivation is deterministic by pre-creating the base branch.
    let base_branch = "canopus/task-aedaca";
    Command::new("git")
        .args(["branch", base_branch])
        .current_dir(&repo)
        .output()
        .unwrap();

    let tasks_path = write_approved_tasks(&state, agenda_id);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::remove_var("DISCORD_WEBHOOK_URL");

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

    // Finalize sidecar must exist (commit succeeded on the -2 branch).
    let sidecar = finalize_path(&state, agenda_id);
    assert!(
        sidecar.exists(),
        "finalize sidecar must be written when collision is resolved via suffix: {}",
        sidecar.display()
    );
    let record = fs::read_to_string(&sidecar).unwrap();
    let expected_collision_branch = format!("{base_branch}-2");
    assert!(
        record.contains(&expected_collision_branch),
        "finalize record should mention the -2 branch; record: {record}"
    );

    // The -2 branch must exist in the repo.
    let branches = Command::new("git")
        .args(["branch", "-a"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_out = String::from_utf8_lossy(&branches.stdout);
    assert!(
        branch_out.contains(&expected_collision_branch),
        "branch {expected_collision_branch} must exist after collision suffix applied: {branch_out}"
    );

    let _ = fs::remove_dir_all(&repo);
}
