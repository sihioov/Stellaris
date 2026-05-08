use crate::adapters::github::GitHubClient;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::cli::args::{derive_state_with_source, FinalizeArgs, WatchArgs};
use crate::cli::commands::delivery_finalize::DeliveryGateReport;
use crate::core::{derive_run_identity, CanopusError, CanopusResult};
use crate::ports::ToolGateway;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) async fn watch(args: &[String]) -> CanopusResult<()> {
    let parsed = WatchArgs::parse(args)?;
    let tools = LocalToolGateway;
    let interval = std::time::Duration::from_secs(10);

    log::info!(
        "[watch] Processed 태스크 폴링 시작 ({})",
        parsed.tasks_path.display()
    );
    loop {
        match approved_processed_tasks(&parsed.tasks_path) {
            Ok(tasks) if !tasks.is_empty() => {
                for task in tasks {
                    let run_id = derive_run_identity(Some(&task.run_id), None)?;
                    let (task_state, state_source) =
                        derive_state_with_source(task.repo_path.as_deref(), &parsed.state);
                    log::info!(
                        "[watch] finalize sidecar persist: {} (source={})",
                        task_state.display(),
                        state_source.as_str()
                    );
                    let finalize_path = finalize_record_path(&task_state, &run_id);
                    // PR-B migration-window idempotency guard (plan §5.1 / §5.4):
                    // when payload-derived state diverges from the watch-side
                    // --state argument, a previous release may have persisted
                    // the finalize sidecar at the legacy location. Honor it so
                    // we never re-finalize during the migration window. Remove
                    // this fallback one release after PR-B lands.
                    let legacy_finalize_path = (parsed.state != task_state)
                        .then(|| finalize_record_path(&parsed.state, &run_id));
                    if finalize_path.exists() {
                        if let Err(e) = persist_delivery_gate_report_if_absent(&task_state, &run_id)
                        {
                            log::error!(
                                "[watch] delivery gate sidecar persist failed {}: {e}",
                                task.task_id
                            );
                        }
                        log::info!(
                            "[watch] finalize record exists; skipping {} ({})",
                            task.task_id,
                            finalize_path.display()
                        );
                        continue;
                    }
                    if let Some(legacy) = legacy_finalize_path.as_ref().filter(|path| path.exists())
                    {
                        log::info!(
                            "[watch] legacy finalize record present at {}; skipping {} (PR-B migration-window guard)",
                            legacy.display(),
                            task.task_id
                        );
                        if let Err(e) = backfill_finalize_record(legacy, &finalize_path) {
                            log::warn!(
                                "[watch] failed to backfill legacy finalize record to {}: {e}",
                                finalize_path.display()
                            );
                        }
                        if let Err(e) = persist_delivery_gate_report_if_absent(&task_state, &run_id)
                        {
                            log::error!(
                                "[watch] delivery gate sidecar persist failed {}: {e}",
                                task.task_id
                            );
                        }
                        continue;
                    }

                    log::info!("[watch] Processed 태스크 발견: {}", task.task_id);
                    let branch = format!("canopus/{run_id}");
                    let repo = task.repo_path.as_deref().unwrap_or(&parsed.repo);
                    match post_approval(repo, &branch, &run_id, None, &tools, FinalizeMode::DryRun)
                        .await
                        .and_then(|output| {
                            persist_finalize_record_if_absent(&task_state, &run_id, &output)
                        })
                        .and_then(|()| persist_delivery_gate_report_if_absent(&task_state, &run_id))
                    {
                        Ok(()) => log::info!(
                            "[watch] finalize record persisted for {} ({})",
                            task.task_id,
                            finalize_path.display()
                        ),
                        Err(e) => log::error!("[watch] finalize 실패 {}: {e}", task.task_id),
                    }
                }
            }
            Ok(_) => {}
            Err(e) => log::warn!("[watch] fetch_processed 실패: {e}"),
        }
        if parsed.once {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApprovedProcessedTask {
    task_id: String,
    run_id: String,
    repo_path: Option<PathBuf>,
}

fn approved_processed_tasks(tasks_path: &Path) -> CanopusResult<Vec<ApprovedProcessedTask>> {
    let text = match fs::read_to_string(tasks_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|e| CanopusError::InvalidInput(format!("invalid tasks JSON: {e}")))?;
    let tasks = value
        .as_array()
        .ok_or_else(|| CanopusError::InvalidInput("tasks JSON must be an array".to_string()))?;

    let mut approved = Vec::new();
    for task in tasks {
        let Some(object) = task.as_object() else {
            continue;
        };
        if object
            .get("meta")
            .and_then(|meta| meta.get("status"))
            .and_then(Value::as_str)
            != Some("Processed")
        {
            continue;
        }
        let Some(task_id) = object
            .get("task_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let Some(payload) = decoded_payload(object.get("payload")) else {
            log::info!("[watch] Processed task {task_id} skipped: missing JSON payload");
            continue;
        };
        if payload.get("approval_state").and_then(Value::as_str) != Some("approved") {
            log::info!("[watch] Processed task {task_id} skipped: approval_state not approved");
            continue;
        }
        let Some(finalize_requested_at) = payload
            .get("finalize_requested_at")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        else {
            log::info!("[watch] Processed task {task_id} skipped: finalize not requested");
            continue;
        };
        let _ = finalize_requested_at;
        let run_id = payload
            .get("agenda_id")
            .or_else(|| payload.get("canopus_agenda_id"))
            .or_else(|| payload.get("run_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(task_id)
            .to_string();
        let repo_path = payload
            .get("repo_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        approved.push(ApprovedProcessedTask {
            task_id: task_id.to_string(),
            run_id,
            repo_path,
        });
    }
    Ok(approved)
}

fn decoded_payload(value: Option<&Value>) -> Option<serde_json::Map<String, Value>> {
    match value? {
        Value::Object(object) => Some(object.clone()),
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|parsed| parsed.as_object().cloned()),
        _ => None,
    }
}

pub(crate) async fn finalize(args: &[String]) -> CanopusResult<()> {
    let parsed = FinalizeArgs::parse(args)?;
    let tools = LocalToolGateway;
    let run_id = derive_run_identity(parsed.agenda_id.as_deref(), parsed.task_id.as_deref())?;
    let branch = format!("canopus/{run_id}");
    let mode = if parsed.allow_mutation {
        FinalizeMode::Mutate
    } else {
        FinalizeMode::DryRun
    };

    let output = post_approval(
        &parsed.repo,
        &branch,
        &run_id,
        parsed.github_issue_number,
        &tools,
        mode,
    )
    .await?;

    let state = parsed.state.unwrap_or_else(|| parsed.repo.join(".canopus"));
    persist_finalize_record(&state, &run_id, &output)?;
    persist_delivery_gate_report_if_absent(&state, &run_id)?;
    println!("{output}");
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizeMode {
    DryRun,
    Mutate,
}

pub(crate) async fn post_approval(
    repo: &std::path::Path,
    branch: &str,
    agenda_id: &str,
    qa_issue_number: Option<u64>,
    tools: &LocalToolGateway,
    mode: FinalizeMode,
) -> CanopusResult<String> {
    let mut plan = vec![
        format!("finalize mode: {:?}", mode),
        format!("agenda_id: {agenda_id}"),
        format!("branch: {branch}"),
    ];

    if matches!(mode, FinalizeMode::DryRun) {
        let diff = tools.changed_files(repo)?;
        plan.push(format!("dry-run changed files:\n{}", diff.stdout));
        plan.push(
            "dry-run: skipped git add/commit/push, gh pr create, and issue close".to_string(),
        );
        notify_discord("🧪 **Finalize dry-run** — no push/PR/GitHub mutation performed.");
        return Ok(plan.join("\n"));
    }

    tools.run_check(repo, &["git", "add", "-A"])?;
    let msg = format!("canopus: complete agenda {}", agenda_id);
    let commit = tools.run_check(repo, &["git", "commit", "-m", &msg])?;
    if commit.status != 0 {
        let no_changes = commit.stdout.contains("nothing to commit")
            || commit.stderr.contains("nothing to commit")
            || commit.stdout.contains("no changes added")
            || commit.stderr.contains("no changes added");
        if no_changes {
            plan.push("commit: no changes to commit; continuing idempotently".to_string());
        } else {
            return Err(CanopusError::Tool(commit.stderr));
        }
    }
    tools.run_check(repo, &["git", "push", "-u", "origin", branch])?;
    let pr_title = format!("[Canopus] {}", agenda_id);
    let pr_body = format!("Automated PR for agenda `{}`", agenda_id);
    tools.run_check(
        repo,
        &[
            "gh", "pr", "create", "--title", &pr_title, "--body", &pr_body, "--base", "main",
        ],
    )?;

    if live_mutations_enabled() {
        if let Some(issue_n) = qa_issue_number {
            if let Some(gh) = GitHubClient::from_env() {
                let _ = gh.close_issue(issue_n);
            }
        }
    } else if qa_issue_number.is_some() {
        log::info!("[watch] dry-run mode: Q&A issue close skipped");
    }

    if live_mutations_enabled() {
        notify_discord("🚀 **PR 생성 완료** — GitHub에서 Approve 후 merge 해주세요.");
    } else {
        notify_discord(
            "🧪 **PR dry-run 완료** — push/PR/issue close는 CANOPUS_ENABLE_LIVE_MUTATIONS=1 없이는 실행하지 않습니다.",
        );
    }
    plan.push("mutate: git push and gh pr create executed".to_string());
    Ok(plan.join("\n"))
}

fn persist_finalize_record(state: &Path, run_id: &str, output: &str) -> CanopusResult<()> {
    let path = finalize_record_path(state, run_id);
    if let Some(runs_dir) = path.parent() {
        fs::create_dir_all(runs_dir)?;
    }
    fs::write(path, output)?;
    Ok(())
}

fn persist_finalize_record_if_absent(
    state: &Path,
    run_id: &str,
    output: &str,
) -> CanopusResult<()> {
    let path = finalize_record_path(state, run_id);
    if path.exists() {
        log::info!(
            "[watch] finalize record exists after dry-run; preserving {}",
            path.display()
        );
        return Ok(());
    }
    if let Some(runs_dir) = path.parent() {
        fs::create_dir_all(runs_dir)?;
    }
    fs::write(path, output)?;
    Ok(())
}

fn finalize_record_path(state: &Path, run_id: &str) -> PathBuf {
    state.join("runs").join(format!("{run_id}-finalize.txt"))
}

/// PR-B migration-window helper: copy a legacy finalize record found under
/// the watch-side `--state` to its payload-derived location, so subsequent
/// watch loops short-circuit at `finalize_path.exists()` without re-checking
/// the legacy path. No-op when target already exists. Plan §5.1 / §5.4.
fn backfill_finalize_record(legacy: &Path, target: &Path) -> CanopusResult<()> {
    if target.exists() {
        return Ok(());
    }
    if let Some(runs_dir) = target.parent() {
        fs::create_dir_all(runs_dir)?;
    }
    fs::copy(legacy, target)?;
    Ok(())
}

fn persist_delivery_gate_report_if_absent(state: &Path, run_id: &str) -> CanopusResult<()> {
    let path = delivery_gate_record_path(state, run_id);
    if path.exists() {
        return Ok(());
    }
    if let Some(runs_dir) = path.parent() {
        fs::create_dir_all(runs_dir)?;
    }
    let report = DeliveryGateReport::from_env(true, false, false);
    let json = serde_json::to_vec_pretty(&report)
        .map_err(|e| CanopusError::InvalidInput(format!("delivery gate JSON failed: {e}")))?;
    fs::write(path, json)?;
    Ok(())
}

fn delivery_gate_record_path(state: &Path, run_id: &str) -> PathBuf {
    state
        .join("runs")
        .join(format!("{run_id}-delivery-gate.json"))
}

pub(crate) fn live_mutations_enabled() -> bool {
    std::env::var("CANOPUS_ENABLE_LIVE_MUTATIONS").as_deref() == Ok("1")
}

pub(crate) fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        let _ = ureq::post(&url).send_json(body);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use crate::adapters::tool_gateway::LocalToolGateway;
    use crate::cli::commands::{
        delivery_finalize::DeliveryGateReport, project_register::project_register,
        work_intake::work_intake,
    };
    use std::process::Command;

    fn clear_canopus_env() {
        for key in [
            "CANOPUS_ENABLE_GITHUB",
            "CANOPUS_ENABLE_LIVE_MUTATIONS",
            "CANOPUS_ALLOW_GITHUB_MUTATION",
            "CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION",
            "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION",
            "CANOPUS_ALLOW_GITHUB_REPO_CREATE",
            "CANOPUS_MOCK_GITHUB",
            "CANOPUS_MOCK_GITHUB_PROJECT_SYNC_FAIL",
            "CANOPUS_ALLOW_GITHUB_PR_MUTATION",
            "CANOPUS_ALLOW_GITHUB_MERGE",
            "CANOPUS_ALLOW_DEPLOY",
            "CANOPUS_DEPLOY_ADAPTER",
            "CANOPUS_DEPLOY_ENVIRONMENT",
            "CANOPUS_DEPLOY_COMMAND",
        ] {
            std::env::remove_var(key);
        }
    }

    fn git_repo(name: &str) -> PathBuf {
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
    #[allow(clippy::await_holding_lock)]
    async fn post_approval_uses_dry_run_for_external_mutations_by_default() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::remove_var("CANOPUS_ENABLE_LIVE_MUTATIONS");
        std::env::remove_var("GITHUB_TOKEN");
        std::env::remove_var("GITHUB_OWNER");
        std::env::remove_var("GITHUB_REPO");
        let repo = git_repo("post-approval-dry-run");
        fs::write(repo.join("canopus-output.txt"), "changed\n").unwrap();
        let tools = LocalToolGateway;

        let output = post_approval(
            &repo,
            "canopus/CANOPUS-DRY",
            "CANOPUS-DRY",
            Some(1),
            &tools,
            FinalizeMode::DryRun,
        )
        .await
        .unwrap();

        assert!(output.contains("dry-run: skipped"));
        let log = Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&log.stdout).contains("CANOPUS-DRY"));
        let _ = fs::remove_dir_all(repo);
    }
    #[test]
    fn project_register_fails_closed_without_live_gates() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::remove_var("CANOPUS_ENABLE_GITHUB");
        std::env::remove_var("CANOPUS_ENABLE_LIVE_MUTATIONS");
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION");
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION");
        let repo = git_repo("project-register-gates");
        let args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--github-owner".to_string(),
            "acme".to_string(),
            "--github-repo".to_string(),
            "demo".to_string(),
            "--project-owner-kind".to_string(),
            "org".to_string(),
            "--project-owner".to_string(),
            "acme".to_string(),
            "--json".to_string(),
        ];

        let err = project_register(&args).unwrap_err();

        assert!(err.to_string().contains("CANOPUS_ENABLE_GITHUB=1"));
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn project_register_repo_create_requires_explicit_gate() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION", "1");
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_REPO_CREATE");
        let repo = git_repo("project-register-repo-create-gate");
        let args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--github-owner".to_string(),
            "acme".to_string(),
            "--github-repo".to_string(),
            "demo".to_string(),
            "--project-owner-kind".to_string(),
            "org".to_string(),
            "--project-owner".to_string(),
            "acme".to_string(),
            "--create-github-repo".to_string(),
            "--json".to_string(),
        ];

        let err = project_register(&args).unwrap_err();

        assert!(err
            .to_string()
            .contains("CANOPUS_ALLOW_GITHUB_REPO_CREATE=1"));
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn mock_project_register_and_work_intake_succeed_offline() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MUTATION", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB", "1");
        let repo = git_repo("project-register-mock-success");
        let register_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--github-owner".to_string(),
            "acme".to_string(),
            "--github-repo".to_string(),
            "demo".to_string(),
            "--project-owner-kind".to_string(),
            "org".to_string(),
            "--project-owner".to_string(),
            "acme".to_string(),
            "--json".to_string(),
        ];

        project_register(&register_args).unwrap();

        let registration = serde_json::json!({
            "github_owner":"acme",
            "github_repo":"demo",
            "github_project_id":"PVT_mock_acme_demo",
            "github_project_url":"https://github.com/orgs/acme/projects/1",
            "github_home_issue_number":1
        })
        .to_string();
        let intake_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--registration".to_string(),
            registration,
            "--task-id".to_string(),
            "discord-1".to_string(),
            "--agenda-id".to_string(),
            "agenda-discord-1".to_string(),
            "--request".to_string(),
            "ship it".to_string(),
            "--discord-message-url".to_string(),
            "https://discord.test/msg".to_string(),
            "--json".to_string(),
        ];
        work_intake(&intake_args).unwrap();
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn work_intake_issue_only_mock_does_not_require_project_gate() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MUTATION", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB", "1");
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION");
        let repo = git_repo("work-intake-issue-only");
        let registration = serde_json::json!({
            "github_owner":"acme",
            "github_repo":"demo"
        })
        .to_string();
        let intake_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--registration".to_string(),
            registration,
            "--task-id".to_string(),
            "discord-issue-only".to_string(),
            "--agenda-id".to_string(),
            "agenda-discord-issue-only".to_string(),
            "--request".to_string(),
            "create issue only".to_string(),
            "--project-sync".to_string(),
            "best-effort".to_string(),
            "--json".to_string(),
        ];

        work_intake(&intake_args).unwrap();

        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn work_intake_requires_real_owner_repo_registration() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MUTATION", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB", "1");
        let repo = git_repo("work-intake-no-owner-repo");
        let registration = serde_json::json!({"github_project_id":"PVT_1"}).to_string();
        let intake_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--registration".to_string(),
            registration,
            "--task-id".to_string(),
            "discord-missing".to_string(),
            "--agenda-id".to_string(),
            "agenda-discord-missing".to_string(),
            "--request".to_string(),
            "missing owner repo".to_string(),
            "--json".to_string(),
        ];

        let err = work_intake(&intake_args).unwrap_err();

        assert!(err.to_string().contains("github_owner"));
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn work_intake_required_project_sync_preflights_before_issue_creation() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MUTATION", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB", "1");
        let repo = git_repo("work-intake-required-preflight");
        let registration = serde_json::json!({
            "github_owner":"acme",
            "github_repo":"demo"
        })
        .to_string();
        let intake_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--registration".to_string(),
            registration,
            "--task-id".to_string(),
            "discord-required".to_string(),
            "--agenda-id".to_string(),
            "agenda-discord-required".to_string(),
            "--request".to_string(),
            "must sync project".to_string(),
            "--project-sync".to_string(),
            "required".to_string(),
            "--json".to_string(),
        ];

        let err = work_intake(&intake_args).unwrap_err();

        assert!(err.to_string().contains("--project-sync required"));
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn work_intake_reports_partial_failure_after_issue_creation() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("CANOPUS_ENABLE_GITHUB", "1");
        std::env::set_var("CANOPUS_ENABLE_LIVE_MUTATIONS", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MUTATION", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB", "1");
        std::env::set_var("CANOPUS_MOCK_GITHUB_PROJECT_SYNC_FAIL", "1");
        let repo = git_repo("work-intake-partial-failure");
        let registration = serde_json::json!({
            "github_owner":"acme",
            "github_repo":"demo",
            "github_project_id":"PVT_1",
            "github_project_status":"Issue Created"
        })
        .to_string();
        let intake_args = vec![
            "--repo".to_string(),
            repo.display().to_string(),
            "--registration".to_string(),
            registration,
            "--task-id".to_string(),
            "discord-partial".to_string(),
            "--agenda-id".to_string(),
            "agenda-discord-partial".to_string(),
            "--request".to_string(),
            "partial fail".to_string(),
            "--json".to_string(),
        ];

        let err = work_intake(&intake_args).unwrap_err();

        assert!(err
            .to_string()
            .contains("GitHub Project sync failed after Issue creation"));
        let _ = fs::remove_dir_all(repo);
        clear_canopus_env();
    }

    #[test]
    fn approved_processed_tasks_requires_approval_and_finalize_evidence() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("canopus-approved-processed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let tasks_path = root.join("tasks.json");
        fs::write(
            &tasks_path,
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "task_id":"discord-string",
                    "payload": serde_json::to_string(&serde_json::json!({
                        "agenda_id":"agenda-discord-string",
                        "approval_state":"approved",
                        "finalize_requested_at":"2026-05-05T00:00:00Z"
                    })).unwrap(),
                    "meta":{"status":"Processed"}
                },
                {
                    "task_id":"discord-object",
                    "payload": {
                        "canopus_agenda_id":"agenda-discord-object",
                        "approval_state":"approved",
                        "finalize_requested_at":"2026-05-05T00:00:01Z"
                    },
                    "meta":{"status":"Processed"}
                },
                {
                    "task_id":"discord-no-finalize",
                    "payload": {
                        "agenda_id":"agenda-discord-no-finalize",
                        "approval_state":"approved"
                    },
                    "meta":{"status":"Processed"}
                },
                {
                    "task_id":"discord-not-approved",
                    "payload": {
                        "agenda_id":"agenda-discord-not-approved",
                        "approval_state":"pending",
                        "finalize_requested_at":"2026-05-05T00:00:02Z"
                    },
                    "meta":{"status":"Processed"}
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let tasks = approved_processed_tasks(&tasks_path).unwrap();

        assert_eq!(
            tasks,
            vec![
                ApprovedProcessedTask {
                    task_id: "discord-string".to_string(),
                    run_id: "agenda-discord-string".to_string(),
                    repo_path: None
                },
                ApprovedProcessedTask {
                    task_id: "discord-object".to_string(),
                    run_id: "agenda-discord-object".to_string(),
                    repo_path: None
                },
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn approved_processed_tasks_carries_payload_repo_path() {
        let root =
            std::env::temp_dir().join(format!("canopus-approved-repo-path-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let tasks_path = root.join("tasks.json");
        fs::write(
            &tasks_path,
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "task_id":"discord-object",
                    "payload": {
                        "agenda_id":"agenda-discord-object",
                        "approval_state":"approved",
                        "finalize_requested_at":"2026-05-05T00:00:01Z",
                        "repo_path":"/tmp/payload-repo"
                    },
                    "meta":{"status":"Processed"}
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let tasks = approved_processed_tasks(&tasks_path).unwrap();

        assert_eq!(
            tasks,
            vec![ApprovedProcessedTask {
                task_id: "discord-object".to_string(),
                run_id: "agenda-discord-object".to_string(),
                repo_path: Some(PathBuf::from("/tmp/payload-repo"))
            }]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn watch_finalization_uses_payload_repo_over_parsed_repo() {
        let fallback_repo = git_repo("watch-fallback-repo");
        let payload_repo = git_repo("watch-payload-repo");
        fs::write(fallback_repo.join("fallback.txt"), "fallback change\n").unwrap();
        fs::write(payload_repo.join("payload.txt"), "payload change\n").unwrap();
        let state = std::env::temp_dir().join(format!(
            "canopus-watch-payload-state-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&state);
        fs::create_dir_all(&state).unwrap();
        let tasks_path = state.join("tasks.json");
        fs::write(
            &tasks_path,
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "task_id":"discord-payload",
                    "payload": {
                        "agenda_id":"agenda-payload",
                        "approval_state":"approved",
                        "finalize_requested_at":"2026-05-05T00:00:01Z",
                        "repo_path": payload_repo.display().to_string()
                    },
                    "meta":{"status":"Processed"}
                }
            ]))
            .unwrap(),
        )
        .unwrap();
        let args = vec![
            "--repo".to_string(),
            fallback_repo.display().to_string(),
            "--state".to_string(),
            state.display().to_string(),
            "--once".to_string(),
            tasks_path.display().to_string(),
        ];

        watch(&args).await.unwrap();

        // PR-A A4: sidecar must land under <payload_repo>/.canopus, not the
        // watch-side --state argument (plan §5.3 / §6.5).
        let payload_state = payload_repo.join(".canopus");
        let record =
            fs::read_to_string(finalize_record_path(&payload_state, "agenda-payload")).unwrap();
        assert!(record.contains("payload.txt"));
        assert!(!record.contains("fallback.txt"));
        assert!(
            !finalize_record_path(&state, "agenda-payload").exists(),
            "finalize record must NOT land under fallback --state when payload.repo_path is present"
        );
        let _ = fs::remove_dir_all(&payload_state);
        let _ = fs::remove_dir_all(state);
        let _ = fs::remove_dir_all(fallback_repo);
        let _ = fs::remove_dir_all(payload_repo);
    }

    #[test]
    fn delivery_gate_denies_merge_and_deploy_until_all_gates_pass() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_PR_MUTATION");
        std::env::remove_var("CANOPUS_ALLOW_GITHUB_MERGE");
        std::env::remove_var("CANOPUS_ALLOW_DEPLOY");
        std::env::remove_var("CANOPUS_DEPLOY_ADAPTER");
        std::env::remove_var("CANOPUS_DEPLOY_ENVIRONMENT");
        std::env::remove_var("CANOPUS_DEPLOY_COMMAND");
        let denied = DeliveryGateReport::from_env(false, false, false);
        assert!(!denied.can_merge());
        assert!(!denied.can_deploy());
        assert!(denied.denial_reason().contains("Discord approval missing"));

        std::env::set_var("CANOPUS_ALLOW_GITHUB_PR_MUTATION", "1");
        std::env::set_var("CANOPUS_ALLOW_GITHUB_MERGE", "1");
        std::env::set_var("CANOPUS_ALLOW_DEPLOY", "1");
        std::env::set_var("CANOPUS_DEPLOY_ADAPTER", "command");
        std::env::set_var("CANOPUS_DEPLOY_ENVIRONMENT", "staging");
        std::env::set_var("CANOPUS_DEPLOY_COMMAND", "echo deploy");
        let allowed = DeliveryGateReport::from_env(true, true, true);
        assert!(allowed.can_merge());
        assert!(allowed.can_deploy());
        clear_canopus_env();
    }
}
