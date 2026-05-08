//! PR-B integration test: migration script in-flight guard + idempotency.
//!
//! Plan reference: `.omc/plans/canopus-multiproject-state-routing.md` §6.4 +
//! §7 PR-B Acceptance B1/B2/B4/B5 + §5.4 D-2.
//!
//! Exercises the `scripts/migrate-canopus-state.sh` helper end-to-end:
//! 1. Dry-run output classifies stale `runs/*.json` + `artifacts/agenda-*`
//!    deterministically (B1).
//! 2. In-flight predicate matches `apps/canopus/src/cli/finalize.rs:116-128`
//!    composite logic — `PendingReview` and `Processed`+finalize-pending
//!    both surface as in-flight (B5 + plan §5.4 D-2).
//! 3. `--apply --mode=move` without `--force-with-inflight` refuses when
//!    in-flight count > 0, leaves the source tree untouched (B4 + B5).
//! 4. `--apply --mode=move --force-with-inflight` performs the moves and
//!    is idempotent on re-run (Decision Driver #1 — plan §7 PR-B 5-step
//!    verification, steps 3 + 4).
//! 5. Watch loop migration-window guard: legacy finalize.txt at the
//!    `<old_state>/runs/...` location short-circuits re-finalization even
//!    when payload.repo_path routes the new state elsewhere (B2 + plan
//!    §5.1 / §5.4).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn migration_script() -> PathBuf {
    repo_root().join("scripts").join("migrate-canopus-state.sh")
}

fn unique_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "canopus-migration-{name}-{pid}-{nanos}",
        pid = std::process::id(),
        nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

struct Fixture {
    root: PathBuf,
    state_root: PathBuf,
    tasks_path: PathBuf,
    repo_a: PathBuf,
    repo_b: PathBuf,
}

impl Fixture {
    fn build(name: &str) -> Self {
        let root = unique_root(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let state_root = root.join("Stellaris").join(".canopus");
        fs::create_dir_all(state_root.join("artifacts")).unwrap();
        fs::create_dir_all(state_root.join("runs")).unwrap();
        let repo_a = root.join("project-a");
        let repo_b = root.join("project-b");
        fs::create_dir_all(&repo_a).unwrap();
        fs::create_dir_all(&repo_b).unwrap();

        // Stale artifact dirs (one per agenda) and run-record sidecars.
        for dir in [
            "agenda-mig-a-task-0-analyst",
            "agenda-mig-a-task-1-plan",
            "agenda-mig-b-task-0-analyst",
        ] {
            let target = state_root.join("artifacts").join(dir);
            fs::create_dir_all(&target).unwrap();
            fs::write(target.join("runtime-log.md"), "stale\n").unwrap();
        }
        fs::write(
            state_root.join("runs").join("agenda-mig-a.json"),
            "[{\"name\":\"prepare\",\"status\":\"ok\"}]\n",
        )
        .unwrap();
        fs::write(
            state_root.join("runs").join("agenda-mig-b.json"),
            "[{\"name\":\"prepare\",\"status\":\"ok\"}]\n",
        )
        .unwrap();
        fs::write(
            state_root.join("runs").join("agenda-mig-orphan.json"),
            "[{\"name\":\"prepare\",\"status\":\"ok\"}]\n",
        )
        .unwrap();

        let tasks_path = root.join("tasks.json");
        let tasks = serde_json::json!([
            {
                "task_id": "task-mig-a",
                "task_type": {"Custom": "canopus.agent"},
                "payload": serde_json::to_string(&serde_json::json!({
                    "request": "do work",
                    "agenda_id": "agenda-mig-a",
                    "repo_path": repo_a.to_str().unwrap(),
                    "approval_state": "approved",
                    "finalize_requested_at": "2026-05-08T00:00:00Z"
                })).unwrap(),
                "meta": {"status": "Processed"}
            },
            {
                "task_id": "task-mig-b",
                "task_type": {"Custom": "canopus.agent"},
                "payload": serde_json::to_string(&serde_json::json!({
                    "request": "review",
                    "agenda_id": "agenda-mig-b",
                    "repo_path": repo_b.to_str().unwrap(),
                    "approval_state": "pending"
                })).unwrap(),
                "meta": {"status": "PendingReview"}
            },
            {
                "task_id": "task-mig-failed",
                "task_type": {"Custom": "canopus.agent"},
                "payload": serde_json::to_string(&serde_json::json!({
                    "request": "old failed",
                    "agenda_id": "agenda-mig-failed",
                    "repo_path": repo_a.to_str().unwrap()
                })).unwrap(),
                "meta": {"status": "Failed"}
            }
        ]);
        fs::write(&tasks_path, serde_json::to_string_pretty(&tasks).unwrap()).unwrap();

        Self {
            root,
            state_root,
            tasks_path,
            repo_a,
            repo_b,
        }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new("bash")
            .arg(migration_script())
            .args(args)
            .arg(format!("--state-root={}", self.state_root.display()))
            .arg(format!("--tasks-path={}", self.tasks_path.display()))
            .output()
            .expect("script invocation failed")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn dry_run_classifies_artifacts_runs_and_marks_inflight() {
    // B1 + B5: dry-run output is deterministic — it lists every artifact +
    // run record with the resolved target path, plus an in-flight roster.
    let fx = Fixture::build("dry-run-classify");
    let out = fx.run(&["--mode=move"]); // dry-run by default (no --apply)
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "dry-run should succeed:\n{stdout}");
    assert!(
        stdout.contains("classification: artifacts=3 runs_json=3 finalize_txt=0"),
        "missing aggregate counts:\n{stdout}"
    );
    assert!(
        stdout.contains("agenda-mig-a-task-0-analyst")
            && stdout.contains("agenda-mig-a-task-1-plan")
            && stdout.contains("agenda-mig-b-task-0-analyst"),
        "artifact dirs not classified:\n{stdout}"
    );
    assert!(
        stdout.contains("agenda-mig-a.json")
            && stdout.contains("agenda-mig-b.json")
            && stdout.contains("agenda-mig-orphan.json"),
        "run records not classified:\n{stdout}"
    );
    // Orphan run record (no payload mapping) flagged explicitly.
    let orphan_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("agenda-mig-orphan.json"))
        .collect();
    assert!(
        orphan_lines.iter().any(|line| line.contains("ORPHAN")),
        "orphan run record must surface ORPHAN tag:\n{stdout}"
    );
    // In-flight predicate (plan §5.4 D-2) surfaces:
    //  - PendingReview task (active state)
    //  - Processed + approved + finalize_requested + no finalize.txt
    assert!(
        stdout.contains("in-flight tasks detected: 2"),
        "in-flight count must be 2 (PendingReview + Processed-finalize-pending):\n{stdout}"
    );
    assert!(
        stdout.contains("task-mig-a") && stdout.contains("task-mig-b"),
        "both in-flight task ids must be listed:\n{stdout}"
    );
    assert!(
        !stdout.contains("task-mig-failed"),
        "Failed (terminal) tasks must NOT be flagged in-flight:\n{stdout}"
    );

    // Source state untouched.
    assert!(fx
        .state_root
        .join("artifacts/agenda-mig-a-task-0-analyst")
        .is_dir());
    assert!(fx.state_root.join("runs/agenda-mig-a.json").is_file());
}

#[test]
fn apply_refuses_when_inflight_present_without_force() {
    // B4 + B5: --apply --mode=move with in-flight tasks must refuse
    // (non-zero exit) and leave the source tree pristine.
    let fx = Fixture::build("apply-refuses-inflight");
    let out = fx.run(&["--mode=move", "--apply"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !out.status.success(),
        "in-flight + --apply without --force-with-inflight must exit non-zero:\n{stdout}"
    );
    assert!(
        stdout.contains("REFUSING --apply"),
        "refusal banner must surface:\n{stdout}"
    );
    // Nothing moved.
    assert!(fx
        .state_root
        .join("artifacts/agenda-mig-a-task-0-analyst")
        .is_dir());
    assert!(fx.state_root.join("runs/agenda-mig-a.json").is_file());
    assert!(!fx.repo_a.join(".canopus").exists());
    assert!(!fx.repo_b.join(".canopus").exists());
}

#[test]
fn apply_with_force_moves_idempotently() {
    // B5 + Decision Driver #1 (plan §7 PR-B 5-step verification, steps 3+4):
    // --apply --mode=move --force-with-inflight performs the moves; re-run
    // is a no-op (skip lines, summary 0/0/0).
    let fx = Fixture::build("apply-force-move");

    let first = fx.run(&["--mode=move", "--apply", "--force-with-inflight"]);
    let first_out = String::from_utf8_lossy(&first.stdout);
    assert!(first.status.success(), "first apply failed:\n{first_out}");
    assert!(
        first_out.contains("summary: moved=") && first_out.contains("orphaned=1"),
        "first apply summary missing:\n{first_out}"
    );

    // Targets present, sources gone.
    assert!(fx
        .repo_a
        .join(".canopus/artifacts/agenda-mig-a-task-0-analyst")
        .is_dir());
    assert!(fx
        .repo_b
        .join(".canopus/artifacts/agenda-mig-b-task-0-analyst")
        .is_dir());
    assert!(fx.repo_a.join(".canopus/runs/agenda-mig-a.json").is_file());
    assert!(fx.repo_b.join(".canopus/runs/agenda-mig-b.json").is_file());
    assert!(!fx
        .state_root
        .join("artifacts/agenda-mig-a-task-0-analyst")
        .exists());
    assert!(!fx.state_root.join("runs/agenda-mig-a.json").exists());

    // Orphan landed under <state_root>/orphans/<agenda_id>/.
    assert!(fx
        .state_root
        .join("orphans/agenda-mig-orphan/agenda-mig-orphan.json")
        .is_file());

    // Re-run: idempotent (zero moves, zero orphans).
    let second = fx.run(&["--mode=move", "--apply", "--force-with-inflight"]);
    let second_out = String::from_utf8_lossy(&second.stdout);
    assert!(
        second.status.success(),
        "re-run must succeed idempotently:\n{second_out}"
    );
    assert!(
        second_out.contains("summary: moved=0 orphaned=0"),
        "re-run summary must show zero moves:\n{second_out}"
    );
}

#[test]
fn keep_mode_with_apply_is_no_op() {
    // B5 (plan §8.1 default policy): mode=keep + --apply must NEVER mutate
    // even when in-flight count is zero — plan default is "report only".
    let fx = Fixture::build("keep-noop");
    // Force a no-inflight world so we exercise the keep guard, not the
    // inflight refusal path.
    fs::write(
        &fx.tasks_path,
        serde_json::to_string_pretty(&serde_json::json!([])).unwrap(),
    )
    .unwrap();

    let out = fx.run(&["--mode=keep", "--apply"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(out.status.success(), "keep+apply must succeed:\n{stdout}");
    assert!(
        stdout.contains("mode=keep: --apply has no destructive effect"),
        "keep guard banner missing:\n{stdout}"
    );
    assert!(fx
        .state_root
        .join("artifacts/agenda-mig-a-task-0-analyst")
        .is_dir());
    assert!(!fx.repo_a.join(".canopus").exists());
}

#[tokio::test]
async fn watch_skips_finalize_when_legacy_record_exists_under_old_state() {
    // B2 (plan §5.1 / §5.4): the watch loop's PR-B migration-window guard
    // honors a finalize record found under the watch-side --state argument
    // even when payload.repo_path routes the new derived state elsewhere.
    let root = unique_root("watch-legacy-guard");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let payload_repo = root.join("payload-repo");
    fs::create_dir_all(&payload_repo).unwrap();
    Command::new("git")
        .arg("init")
        .current_dir(&payload_repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "canopus@example.invalid"])
        .current_dir(&payload_repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Canopus Test"])
        .current_dir(&payload_repo)
        .output()
        .unwrap();
    fs::write(payload_repo.join("README.md"), "# fixture\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&payload_repo)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&payload_repo)
        .output()
        .unwrap();

    let watch_state = root.join("legacy-state");
    fs::create_dir_all(watch_state.join("runs")).unwrap();
    let legacy_finalize = watch_state.join("runs").join("agenda-legacy-finalize.txt");
    fs::write(&legacy_finalize, "legacy-finalize-record\n").unwrap();

    let tasks_path = watch_state.join("tasks.json");
    fs::write(
        &tasks_path,
        serde_json::to_string_pretty(&serde_json::json!([
            {
                "task_id": "discord-legacy",
                "task_type": {"Custom": "canopus.agent"},
                "payload": serde_json::to_string(&serde_json::json!({
                    "agenda_id": "agenda-legacy",
                    "approval_state": "approved",
                    "finalize_requested_at": "2026-05-08T00:00:00Z",
                    "repo_path": payload_repo.to_str().unwrap()
                })).unwrap(),
                "meta": {"status": "Processed"}
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let args = vec![
        "canopus".to_string(),
        "watch".to_string(),
        "--repo".to_string(),
        payload_repo.display().to_string(),
        "--state".to_string(),
        watch_state.display().to_string(),
        "--once".to_string(),
        tasks_path.display().to_string(),
    ];
    canopus::cli::run(args).await.unwrap();

    // New-location finalize.txt under <payload_repo>/.canopus must NOT
    // contain a freshly-generated dry-run plan; it should either be absent
    // or be the backfilled copy of the legacy record.
    let new_finalize = payload_repo
        .join(".canopus")
        .join("runs")
        .join("agenda-legacy-finalize.txt");
    if new_finalize.exists() {
        let body = fs::read_to_string(&new_finalize).unwrap();
        assert!(
            body.contains("legacy-finalize-record"),
            "backfilled finalize record must mirror legacy contents, got:\n{body}"
        );
        assert!(
            !body.contains("finalize mode"),
            "watch must NOT regenerate a dry-run plan when legacy record exists: {body}"
        );
    }
    // Legacy record preserved untouched.
    let legacy_after = fs::read_to_string(&legacy_finalize).unwrap();
    assert_eq!(legacy_after, "legacy-finalize-record\n");

    let _ = fs::remove_dir_all(&root);
}
