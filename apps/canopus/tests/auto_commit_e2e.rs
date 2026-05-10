//! AC5/AC7: end-to-end auto-commit flow via the public `canopus watch --once` entry point.
//!
//! AC5: gate ON + approval state → branch created + commit made + finalize sidecar written
//! AC7: rollback — `git push` is NEVER called (verified by absence of origin remote; a push
//!      attempt would fail and surface as a test failure)
#![allow(clippy::await_holding_lock)]

use canopus::cli;
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::{LazyLock, Mutex},
};

/// Serialises env mutation across tests in this file.
static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

struct EnvRestore {
    values: Vec<(&'static str, Option<String>)>,
}

impl EnvRestore {
    fn capture(keys: &[&'static str]) -> Self {
        Self {
            values: keys.iter().map(|k| (*k, env::var(k).ok())).collect(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            match value {
                Some(v) => env::set_var(key, v),
                None => env::remove_var(key),
            }
        }
    }
}

/// Build a temporary git repo with:
/// - initial commit containing README.md and .gitignore (which ignores .canopus/)
/// - git user identity configured
///
/// Returns the repo root. Caller is responsible for cleanup via `fs::remove_dir_all`.
fn setup_test_repo(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("canopus-e2e-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let run = |args: &[&str]| {
        let out = Command::new(args[0])
            .args(&args[1..])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "setup command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run(&["git", "init"]);
    run(&["git", "config", "user.email", "canopus@example.invalid"]);
    run(&["git", "config", "user.name", "Canopus Test"]);

    // .gitignore must include .canopus/ — required by LocalCommitOnly pre-flight (PM-a)
    fs::write(root.join(".gitignore"), ".canopus/\n").unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();

    run(&["git", "add", ".gitignore", "README.md"]);
    run(&["git", "commit", "-m", "init"]);

    root
}

/// Build a minimal tasks.json for a single approved Processed task.
/// `agenda_id` + `task_id` mirror the submit-time run_id / branch derivation.
fn tasks_json(agenda_id: &str, task_id: &str, repo_path: Option<&str>) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "agenda_id": agenda_id,
        "approval_state": "approved",
        "finalize_requested_at": "2026-05-09T00:00:00Z"
    });
    if let Some(path) = repo_path {
        payload
            .as_object_mut()
            .unwrap()
            .insert("repo_path".into(), serde_json::Value::String(path.into()));
    }
    serde_json::json!([{
        "task_id": task_id,
        "payload": payload.to_string(),   // string-encoded payload (as watch expects)
        "meta": { "status": "Processed" }
    }])
}

// ── AC5 ──────────────────────────────────────────────────────────────────────
//
// Gate ON + approval state → branch created + commit made + finalize sidecar
// contains expected content.
//
// Branch derivation (traced statically):
//   agenda_id = "agenda-e2e-ac5"
//   derive_run_identity(Some("agenda-e2e-ac5")) → "agenda-e2e-ac5" (no transform)
//   derive_branch_name("", "agenda-e2e-ac5")
//     slug = "task" (empty user_request)
//     hex chars from "agenda-e2e-ac5": a,g✗,e,n✗,d,a,-✗,e,2,e,-✗,a,c,5
//       → first 6 hex = a,e,d,a,e,2 → short = "aedae2"
//   → "canopus/task-aedae2"
//
// Subject line (traced statically):
//   changed file "hello.txt" has no prefix match → modules = [] → module_str = "misc"
//   → "[misc] feat: complete agenda agenda-e2e-ac5"

#[tokio::test]
async fn local_commit_only_creates_branch_and_commit() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "CANOPUS_ENABLE_LIVE_MUTATIONS",
        "CANOPUS_ENABLE_GITHUB",
        "CANOPUS_AGENT_RUNTIME",
        "DISCORD_WEBHOOK_URL",
    ]);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "0");
    env::set_var("CANOPUS_ENABLE_GITHUB", "0");
    env::remove_var("CANOPUS_AGENT_RUNTIME"); // ensures runtime = "mock", model = "n/a"
    env::remove_var("DISCORD_WEBHOOK_URL");

    let agenda_id = "agenda-e2e-ac5";
    let task_id = "task-agenda-e2e-ac5";
    let run_id = "agenda-e2e-ac5-task-agenda-e2e-ac5";
    let repo = setup_test_repo("ac5");
    let expected_branch = format!("canopus/{run_id}");
    Command::new("git")
        .args(["checkout", "-b", &expected_branch])
        .current_dir(&repo)
        .output()
        .unwrap();
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // Simulate executor output: write a changed file before finalize
    fs::write(repo.join("hello.txt"), "executor output\n").unwrap();

    // Write tasks.json
    let tasks_path = state.join("tasks.json");
    fs::write(
        &tasks_path,
        serde_json::to_string_pretty(&tasks_json(agenda_id, task_id, None)).unwrap(),
    )
    .unwrap();

    // Invoke watch --once (public API — same path as v1_smoke.rs)
    cli::run(vec![
        "canopus".into(),
        "watch".into(),
        "--repo".into(),
        repo.display().to_string(),
        "--state".into(),
        state.display().to_string(),
        "--once".into(),
        tasks_path.display().to_string(),
    ])
    .await
    .unwrap();

    // ── AC5 assertion 1: finalization stayed on the submit-created branch ────
    let branch_out = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let branch_name = String::from_utf8_lossy(&branch_out.stdout)
        .trim()
        .to_string();

    assert_eq!(
        branch_name, expected_branch,
        "finalization must commit on the existing submit-created branch"
    );

    // ── AC5 assertion 2: commit subject matches AC3 regex ─────────────────────
    let subject_out = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let subject = String::from_utf8_lossy(&subject_out.stdout)
        .trim()
        .to_string();

    assert_eq!(
        subject, "[misc] feat: complete agenda agenda-e2e-ac5-task-agenda-e2e-ac5",
        "commit subject did not match V1.5 expected format"
    );
    // AC3 regex: ^\[[a-z0-9/_-]+\]\s+(feat|fix|refactor|docs|chore|style|test):\s+\S
    assert!(
        subject_regex_matches(&subject),
        "subject '{}' does not satisfy AC3 regex",
        subject
    );

    // ── AC5 assertion 3: trailers present in full commit body ─────────────────
    let body_out = Command::new("git")
        .args(["log", "-1", "--format=%B"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let body = String::from_utf8_lossy(&body_out.stdout).to_string();

    assert!(
        body.contains("Co-Authored-By: Canopus <noreply@stellaris.local>"),
        "commit body missing Co-Authored-By trailer. body:\n{body}"
    );
    assert!(
        body.contains("Agent-Runtime:"),
        "commit body missing Agent-Runtime trailer. body:\n{body}"
    );

    // ── AC5 assertion 4: finalize sidecar was written ─────────────────────────
    let finalize_path = state.join("runs").join(format!("{run_id}-finalize.txt"));
    assert!(
        finalize_path.exists(),
        "finalize sidecar not found at {}",
        finalize_path.display()
    );
    let sidecar = fs::read_to_string(&finalize_path).unwrap();
    assert!(
        sidecar.contains("local-commit-only"),
        "finalize sidecar does not mention local-commit-only. content:\n{sidecar}"
    );
    assert!(
        sidecar.contains(&expected_branch),
        "finalize sidecar does not contain branch name. content:\n{sidecar}"
    );

    // ── AC7 assertion: no push was attempted ─────────────────────────────────
    // No origin remote was added. If push had been attempted it would have
    // returned a tool error that propagated up and failed the cli::run() call
    // above. Passing this point proves no push occurred. Belt-and-suspenders:
    // confirm there is no origin.
    let remote_out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        !remote_out.status.success(),
        "origin remote exists unexpectedly — test fixture misconfigured"
    );

    let _ = fs::remove_dir_all(repo);
}

// ── AC7 (explicit bogus remote variant) ──────────────────────────────────────
//
// Adds a syntactically valid but unreachable remote. Any `git push` attempt
// would fail with a network/DNS error. The test succeeds → no push was made.

#[tokio::test]
async fn local_commit_only_no_push_to_remote() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = EnvRestore::capture(&[
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        "CANOPUS_ENABLE_LIVE_MUTATIONS",
        "CANOPUS_ENABLE_GITHUB",
        "CANOPUS_AGENT_RUNTIME",
        "DISCORD_WEBHOOK_URL",
    ]);

    env::set_var("CANOPUS_ALLOW_LOCAL_COMMIT", "1");
    env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "0");
    env::set_var("CANOPUS_ENABLE_GITHUB", "0");
    env::remove_var("CANOPUS_AGENT_RUNTIME");
    env::remove_var("DISCORD_WEBHOOK_URL");

    let agenda_id = "agenda-e2e-ac7";
    let task_id = "task-agenda-e2e-ac7";
    let run_id = "agenda-e2e-ac7-task-agenda-e2e-ac7";
    let repo = setup_test_repo("ac7");
    let expected_branch = format!("canopus/{run_id}");
    Command::new("git")
        .args(["checkout", "-b", &expected_branch])
        .current_dir(&repo)
        .output()
        .unwrap();
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();

    // Add a bogus remote. If post_approval tried to push it would fail with
    // fatal: repository 'https://bogus.invalid/test.git/' not found
    // which would propagate as CanopusError::Tool and fail cli::run().
    Command::new("git")
        .args(["remote", "add", "origin", "https://bogus.invalid/test.git"])
        .current_dir(&repo)
        .output()
        .unwrap();

    // Executor change
    fs::write(repo.join("feature.txt"), "new feature\n").unwrap();

    let tasks_path = state.join("tasks.json");
    fs::write(
        &tasks_path,
        serde_json::to_string_pretty(&tasks_json(agenda_id, task_id, None)).unwrap(),
    )
    .unwrap();

    // This must succeed. If push were attempted against bogus remote, git would
    // fail and the error would bubble up here.
    cli::run(vec![
        "canopus".into(),
        "watch".into(),
        "--repo".into(),
        repo.display().to_string(),
        "--state".into(),
        state.display().to_string(),
        "--once".into(),
        tasks_path.display().to_string(),
    ])
    .await
    .unwrap();

    // Verify a commit was made (not just a dry-run that silently skipped push)
    let log_out = Command::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let log_line = String::from_utf8_lossy(&log_out.stdout).to_string();
    assert!(
        log_line.contains("feat:"),
        "expected a canopus feat commit in log, got: {log_line}"
    );

    // Verify the commit is local-only: origin/HEAD or any remote-tracking ref
    // for origin must NOT exist.
    let remote_refs = Command::new("git")
        .args(["branch", "-r"])
        .current_dir(&repo)
        .output()
        .unwrap();
    let remote_refs_str = String::from_utf8_lossy(&remote_refs.stdout).to_string();
    assert!(
        remote_refs_str.trim().is_empty(),
        "remote tracking refs found — push may have occurred: {remote_refs_str}"
    );

    let _ = fs::remove_dir_all(repo);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn subject_regex_matches(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('[') else {
        return false;
    };
    let Some(close) = rest.find(']') else {
        return false;
    };
    let bracket_content = &rest[..close];
    if bracket_content.is_empty()
        || !bracket_content
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_' || c == '-')
    {
        return false;
    }
    let after_bracket = &rest[close + 1..]; // "] feat: ..."
    let after_bracket = after_bracket.strip_prefix(' ').unwrap_or(after_bracket);
    let valid_types = ["feat", "fix", "refactor", "docs", "chore", "style", "test"];
    for t in valid_types {
        let prefix = format!("{t}: ");
        if let Some(after_type) = after_bracket.strip_prefix(&prefix) {
            return after_type
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace());
        }
    }
    false
}
