//! PR-A A2 integration test: multi-project state routing.
//!
//! Plan reference: `.omc/plans/canopus-multiproject-state-routing.md`
//! §6.2 (integration) + §7 PR-A acceptance A2/A6.
//!
//! Two fixture repos exercise:
//! 1. payload `repo_path=A` → `--state <A>/.canopus` (payload_repo source)
//! 2. payload `repo_path=B` → `--state <B>/.canopus` (independent of A)
//! 3. self-hosting (Stellaris on Stellaris) → equal payload + env paths
//! 4. missing payload → env_state fallback (principle #3 backwards compat)
//! 5. canopus submit end-to-end with payload-derived --repo lands artifacts
//!    + run records in the *payload* repo (A6: mock runtime CWD wiring).

use canopus::cli::{self, derive_state_for_run, derive_state_with_source, StateSource};
use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};
use laniakea::handlers::custom::canopus_submit_args_for_label;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
}

fn git_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "canopus-multiproject-{name}-{}",
        std::process::id()
    ));
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

fn payload_with_repo(repo: &Path, request: &str) -> String {
    serde_json::json!({
        "request": request,
        "repo_path": repo.to_str().unwrap(),
    })
    .to_string()
}

fn task_message(task_id: &str, payload: String) -> TaskMessage {
    TaskMessage {
        task_id: task_id.to_string(),
        task_type: TaskType::Custom("canopus.agent".to_string()),
        payload,
        meta: TaskMeta::default(),
    }
}

#[test]
fn submit_args_route_state_per_project_repo_path() {
    // Two independent fixture repos must each get their own `<repo>/.canopus`
    // resolved as the `--state` argument when the payload metadata routes
    // them.
    let repo_a = git_repo("project-a");
    let repo_b = git_repo("project-b");

    let args_a = canopus_submit_args_for_label(
        &task_message("disc-a", payload_with_repo(&repo_a, "do A work")),
        "/env-repo",
        "/env-state",
        "canopus.agent",
    )
    .unwrap();
    let args_b = canopus_submit_args_for_label(
        &task_message("disc-b", payload_with_repo(&repo_b, "do B work")),
        "/env-repo",
        "/env-state",
        "canopus.agent",
    )
    .unwrap();

    assert_eq!(arg_value(&args_a, "--repo"), Some(repo_a.to_str().unwrap()));
    assert_eq!(
        arg_value(&args_a, "--state"),
        Some(repo_a.join(".canopus").to_str().unwrap()),
        "project A submit must route state to <A>/.canopus"
    );
    assert_eq!(arg_value(&args_b, "--repo"), Some(repo_b.to_str().unwrap()));
    assert_eq!(
        arg_value(&args_b, "--state"),
        Some(repo_b.join(".canopus").to_str().unwrap()),
        "project B submit must route state to <B>/.canopus"
    );
    assert_ne!(
        arg_value(&args_a, "--state"),
        arg_value(&args_b, "--state"),
        "two project submits must produce disjoint --state arguments"
    );

    let _ = fs::remove_dir_all(&repo_a);
    let _ = fs::remove_dir_all(&repo_b);
}

#[test]
fn submit_args_fall_back_to_env_state_without_payload_repo_path() {
    // Plan principle #3 backwards compat: omitted payload repo → env_state
    // preserved untouched.
    let payload = serde_json::json!({"request": "no payload routing"}).to_string();
    let args = canopus_submit_args_for_label(
        &task_message("disc-no-route", payload),
        "/env-repo",
        "/env-state",
        "canopus.agent",
    )
    .unwrap();

    assert_eq!(arg_value(&args, "--repo"), Some("/env-repo"));
    assert_eq!(arg_value(&args, "--state"), Some("/env-state"));
}

#[test]
fn self_hosting_payload_yields_same_state_as_env() {
    // Plan §8.2: when payload_repo points at the same tree the env_state
    // already lives under (Stellaris-on-Stellaris), derivation must produce
    // the same path so single-project runs behave identically.
    let repo = git_repo("self-hosting");
    let env_state = repo.join(".canopus");

    let (resolved, source) = derive_state_with_source(Some(&repo), &env_state);
    assert_eq!(resolved, env_state);
    assert_eq!(source, StateSource::PayloadRepo);

    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn derive_helper_signature_matches_plan_pubfn_pathbuf() {
    // PR-A spec §7: `pub fn derive_state_for_run(payload_repo: Option<&Path>,
    // parsed_state: &Path) -> PathBuf`.
    let repo = git_repo("derive-helper-signature");
    let env_state = std::env::temp_dir().join(format!(
        "canopus-multiproject-env-state-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&env_state);

    let resolved: PathBuf = derive_state_for_run(Some(&repo), &env_state);
    assert_eq!(resolved, repo.join(".canopus"));

    let fallback: PathBuf = derive_state_for_run(None, &env_state);
    assert_eq!(fallback, env_state);

    let _ = fs::remove_dir_all(&repo);
}

#[tokio::test]
async fn submit_lands_artifacts_under_payload_repo_state_via_mock_runtime() {
    // PR-A A6: mock runtime CWD = payload repo, artifacts land under the
    // payload-derived state. Exercises canopus::cli::run end-to-end so the
    // command + codex runtime call sites at command.rs:38 / codex.rs:101
    // (which both honor `context.repo_path`) are protected against
    // regression.
    let repo = git_repo("submit-mock-cwd");
    let payload_state = repo.join(".canopus");

    cli::run(vec![
        "canopus".to_string(),
        "submit".to_string(),
        "--repo".to_string(),
        repo.display().to_string(),
        "--state".to_string(),
        payload_state.display().to_string(),
        "--agenda-id".to_string(),
        "MULTIPROJECT-1".to_string(),
        "exercise multi-project routing".to_string(),
    ])
    .await
    .unwrap();

    // Mock runtime writes canopus-mock-output.txt into context.repo_path.
    assert!(
        repo.join("canopus-mock-output.txt").exists(),
        "mock runtime CWD must equal payload repo"
    );
    // Artifacts + run records must land under the payload-derived state, not
    // a sibling project state directory.
    assert!(payload_state.join("artifacts").is_dir());
    assert!(payload_state
        .join("runs")
        .join("multiproject-1.json")
        .exists());

    let _ = fs::remove_dir_all(&repo);
}
