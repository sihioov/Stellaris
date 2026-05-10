use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn canopus_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_canopus"))
}

fn git_repo(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!(
        "canopus-finalize-approved-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    for args in [
        vec!["git", "init"],
        vec!["git", "config", "user.email", "canopus@example.invalid"],
        vec!["git", "config", "user.name", "Canopus Test"],
    ] {
        let out = Command::new(args[0])
            .args(&args[1..])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fs::write(root.join(".gitignore"), ".canopus/\n").unwrap();
    fs::write(root.join("README.md"), "# fixture\n").unwrap();
    let out = Command::new("git")
        .args(["add", ".gitignore", "README.md"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(out.status.success());
    root
}

fn write_tasks(path: &Path, repo: &Path, task_id: &str, agenda_id: &str) {
    write_tasks_with_payload(
        path,
        task_id,
        serde_json::json!({
            "agenda_id": agenda_id,
            "approval_state": "approved",
            "finalize_requested_at": "2026-05-10T00:00:00Z",
            "repo_path": repo.display().to_string()
        }),
    );
}

fn write_tasks_with_payload(path: &Path, task_id: &str, payload: serde_json::Value) {
    let tasks = serde_json::json!([{
        "task_id": task_id,
        "payload": payload,
        "meta": { "status": "Processed" }
    }]);
    fs::write(path, serde_json::to_string_pretty(&tasks).unwrap()).unwrap();
}

fn write_duplicate_tasks(path: &Path, repo: &Path, task_id: &str, agenda_id: &str) {
    let task = serde_json::json!({
        "task_id": task_id,
        "payload": {
            "agenda_id": agenda_id,
            "approval_state": "approved",
            "finalize_requested_at": "2026-05-10T00:00:00Z",
            "repo_path": repo.display().to_string()
        },
        "meta": { "status": "Processed" }
    });
    let tasks = serde_json::json!([task.clone(), task]);
    fs::write(path, serde_json::to_string_pretty(&tasks).unwrap()).unwrap();
}

fn run_finalize(tasks: &Path, task_id: &str, allow_local_commit: bool) -> std::process::Output {
    let mut cmd = Command::new(canopus_bin());
    cmd.args([
        "finalize-approved",
        "--tasks",
        tasks.to_str().unwrap(),
        "--task-id",
        task_id,
        "--json",
    ]);
    cmd.env(
        "CANOPUS_ALLOW_LOCAL_COMMIT",
        if allow_local_commit { "1" } else { "0" },
    );
    cmd.env_remove("DISCORD_WEBHOOK_URL");
    cmd.env_remove("CANOPUS_REPO");
    cmd.env_remove("CANOPUS_REPO_PATH");
    cmd.env_remove("CANOPUS_STATE");
    cmd.env_remove("CANOPUS_STATE_PATH");
    cmd.current_dir(tasks.parent().unwrap());
    cmd.output().unwrap()
}

fn checkout_branch(repo: &Path, branch: &str) {
    let checkout = Command::new("git")
        .args(["checkout", "-b", branch])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        checkout.status.success(),
        "{}",
        String::from_utf8_lossy(&checkout.stderr)
    );
}

fn parse_stdout_json(out: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|err| {
        panic!(
            "stdout was not JSON: {err}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn finalize_approved_gate_disabled_emits_single_dry_run_json() {
    let repo = git_repo("dry-run");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(
        &tasks,
        &repo,
        "discord-8c04ecc8c056",
        "agenda-discord-8c04ecc8c056",
    );

    let out = run_finalize(&tasks, "discord-8c04ecc8c056", false);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["status"], "dry_run");
    assert_eq!(json["mode"], "dry_run");
    assert_eq!(
        json["branch"],
        "canopus/agenda-discord-8c04ecc8c056-discord-8c04ecc8c056"
    );

    let subject = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&subject.stdout).trim(), "init");
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_repeated_dry_run_stays_dry_run_not_already_finalized() {
    let repo = git_repo("dry-run-repeat");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, "discord-repeat", "agenda-discord-repeat");

    let first = run_finalize(&tasks, "discord-repeat", false);
    assert!(first.status.success());
    assert_eq!(parse_stdout_json(&first)["status"], "dry_run");

    let second = run_finalize(&tasks, "discord-repeat", false);
    assert!(second.status.success());
    let json = parse_stdout_json(&second);
    assert_eq!(json["status"], "dry_run");
    assert_eq!(json["idempotent"], true);

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_commits_on_existing_discord_submit_branch() {
    let repo = git_repo("commit");
    let task_id = "discord-8c04ecc8c056";
    let agenda_id = "agenda-discord-8c04ecc8c056";
    let expected_branch = "canopus/agenda-discord-8c04ecc8c056-discord-8c04ecc8c056";
    checkout_branch(&repo, expected_branch);
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(state.join("runs")).unwrap();
    fs::write(
        state
            .join("runs")
            .join(format!("{agenda_id}-{task_id}-token-usage.json")),
        serde_json::to_string_pretty(&serde_json::json!({
            "input_tokens": 12_345,
            "output_tokens": 678
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, task_id, agenda_id);

    let out = run_finalize(&tasks, task_id, true);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["status"], "finalized");
    assert_eq!(json["branch"], expected_branch);
    assert!(json["commit"].as_str().unwrap_or("").len() >= 7);
    assert_eq!(json["token_usage"]["input_tokens"], 12_345);
    assert_eq!(json["token_usage"]["output_tokens"], 678);
    assert_eq!(json["token_usage"]["total_tokens"], 13_023);

    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&branch.stdout).trim(),
        expected_branch
    );
    let branches = Command::new("git")
        .args(["branch", "--list", "canopus/task-*"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branches.stdout).trim().is_empty());
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_repeated_local_success_reports_already_finalized() {
    let repo = git_repo("already-finalized");
    let task_id = "discord-repeat-local";
    let agenda_id = "agenda-discord-repeat-local";
    let expected_branch = "canopus/agenda-discord-repeat-local-discord-repeat-local";
    checkout_branch(&repo, expected_branch);
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, task_id, agenda_id);

    let first = run_finalize(&tasks, task_id, true);
    assert!(first.status.success());
    let first_json = parse_stdout_json(&first);
    assert_eq!(first_json["status"], "finalized");
    let first_commit = first_json["commit"].as_str().unwrap().to_string();

    let second = run_finalize(&tasks, task_id, true);
    assert!(second.status.success());
    let second_json = parse_stdout_json(&second);
    assert_eq!(second_json["status"], "already_finalized");
    assert_eq!(second_json["idempotent"], true);
    assert_eq!(second_json["commit"], first_commit);

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_repeated_no_changes_reports_already_finalized() {
    let repo = git_repo("already-no-changes");
    let task_id = "discord-no-changes";
    let agenda_id = "agenda-discord-no-changes";
    let expected_branch = "canopus/agenda-discord-no-changes-discord-no-changes";
    checkout_branch(&repo, expected_branch);
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, task_id, agenda_id);

    let first = run_finalize(&tasks, task_id, true);
    assert!(first.status.success());
    assert_eq!(parse_stdout_json(&first)["status"], "no_changes");

    let second = run_finalize(&tasks, task_id, true);
    assert!(second.status.success());
    let second_json = parse_stdout_json(&second);
    assert_eq!(second_json["status"], "already_finalized");
    assert_eq!(second_json["idempotent"], true);

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_missing_task_emits_ok_false_json_and_nonzero() {
    let repo = git_repo("missing");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, "discord-present", "agenda-discord-present");

    let out = run_finalize(&tasks, "discord-missing", false);
    assert!(!out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "task_not_found");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("not found"));
    assert_eq!(json["error"]["retryable"], true);
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_duplicate_task_id_emits_ambiguous_json_and_nonzero() {
    let repo = git_repo("duplicate");
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_duplicate_tasks(&tasks, &repo, "discord-dup", "agenda-discord-dup");

    let out = run_finalize(&tasks, "discord-dup", false);
    assert!(!out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "ambiguous_task_id");
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_eligibility_failures_emit_structured_json() {
    for (name, payload, expected_code) in [
        (
            "approval-missing",
            serde_json::json!({
                "agenda_id": "agenda-approval-missing",
                "approval_state": "pending",
                "finalize_requested_at": "2026-05-10T00:00:00Z"
            }),
            "approval_missing",
        ),
        (
            "finalize-not-requested",
            serde_json::json!({
                "agenda_id": "agenda-finalize-not-requested",
                "approval_state": "approved"
            }),
            "finalize_not_requested",
        ),
        (
            "repo-path-missing",
            serde_json::json!({
                "agenda_id": "agenda-repo-path-missing",
                "approval_state": "approved",
                "finalize_requested_at": "2026-05-10T00:00:00Z"
            }),
            "repo_path_missing",
        ),
    ] {
        let repo = git_repo(name);
        let state = repo.join(".canopus");
        fs::create_dir_all(&state).unwrap();
        let task_id = format!("discord-{name}");
        let tasks = state.join("tasks.json");
        write_tasks_with_payload(&tasks, &task_id, payload);

        let out = run_finalize(&tasks, &task_id, false);
        assert!(!out.status.success(), "{name} unexpectedly succeeded");
        let json = parse_stdout_json(&out);
        assert_eq!(json["ok"], false, "{name}");
        assert_eq!(json["error"]["code"], expected_code, "{name}");

        let _ = fs::remove_dir_all(repo);
    }
}

#[test]
fn finalize_approved_local_commit_branch_preflight_failures_are_structured() {
    for case in [
        "protected",
        "wrong-branch",
        "dirty-index",
        "missing-canopus-ignore",
    ] {
        let repo = git_repo(case);
        let task_id = format!("discord-{case}");
        let agenda_id = format!("agenda-discord-{case}");
        let expected_branch = format!("canopus/{agenda_id}-{task_id}");
        match case {
            "protected" => {}
            "wrong-branch" => checkout_branch(&repo, "canopus/other"),
            "dirty-index" => {
                checkout_branch(&repo, &expected_branch);
                fs::write(repo.join("staged.txt"), "staged\n").unwrap();
                let add = Command::new("git")
                    .args(["add", "staged.txt"])
                    .current_dir(&repo)
                    .output()
                    .unwrap();
                assert!(add.status.success());
            }
            "missing-canopus-ignore" => {
                checkout_branch(&repo, &expected_branch);
                fs::write(repo.join(".gitignore"), "\n").unwrap();
            }
            _ => unreachable!(),
        }
        fs::write(repo.join("work.txt"), "change\n").unwrap();
        let state = repo.join(".canopus");
        fs::create_dir_all(&state).unwrap();
        let tasks = state.join("tasks.json");
        write_tasks(&tasks, &repo, &task_id, &agenda_id);

        let out = run_finalize(&tasks, &task_id, true);
        assert!(!out.status.success(), "{case} unexpectedly succeeded");
        let json = parse_stdout_json(&out);
        assert_eq!(json["ok"], false, "{case}");
        assert_eq!(json["error"]["code"], "branch_preflight_failed", "{case}");

        let _ = fs::remove_dir_all(repo);
    }
}

#[test]
fn finalize_approved_detached_head_failure_is_structured_json() {
    let repo = git_repo("detached");
    let task_id = "discord-detached";
    let agenda_id = "agenda-discord-detached";
    let detach = Command::new("git")
        .args(["checkout", "--detach", "HEAD"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        detach.status.success(),
        "{}",
        String::from_utf8_lossy(&detach.stderr)
    );
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, task_id, agenda_id);

    let out = run_finalize(&tasks, task_id, true);
    assert!(!out.status.success());
    let json = parse_stdout_json(&out);
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "branch_preflight_failed");

    let _ = fs::remove_dir_all(repo);
}

#[test]
fn finalize_approved_gate_on_upgrades_prior_dry_run_sidecar_to_local_commit() {
    let repo = git_repo("dry-run-upgrade");
    let task_id = "discord-upgrade";
    let agenda_id = "agenda-discord-upgrade";
    let run_id = "agenda-discord-upgrade-discord-upgrade";
    let expected_branch = format!("canopus/{run_id}");
    checkout_branch(&repo, &expected_branch);
    fs::write(repo.join("work.txt"), "change\n").unwrap();
    let state = repo.join(".canopus");
    fs::create_dir_all(&state).unwrap();
    let tasks = state.join("tasks.json");
    write_tasks(&tasks, &repo, task_id, agenda_id);

    let dry_run = run_finalize(&tasks, task_id, false);
    assert!(
        dry_run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let sidecar = state.join("runs").join(format!("{run_id}-finalize.txt"));
    let dry_record = fs::read_to_string(&sidecar).unwrap();
    assert!(dry_record.contains("finalize mode: DryRun"));

    let committed = run_finalize(&tasks, task_id, true);
    assert!(
        committed.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&committed.stdout),
        String::from_utf8_lossy(&committed.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&committed.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["status"], "finalized");
    assert_eq!(json["branch"], expected_branch);
    assert!(json["commit"].as_str().unwrap_or("").len() >= 7);
    let upgraded_record = fs::read_to_string(&sidecar).unwrap();
    assert!(upgraded_record.contains("finalize mode: LocalCommitOnly"));
    assert!(upgraded_record.contains("commit: "));

    let _ = fs::remove_dir_all(repo);
}
