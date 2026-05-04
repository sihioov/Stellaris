use crate::adapters::github::GitHubClient;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::cli::args::{FinalizeArgs, WatchArgs};
use crate::core::{derive_run_identity, CanopusError, CanopusResult};
use crate::ports::ToolGateway;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) async fn watch(args: &[String]) -> CanopusResult<()> {
    use dysonsphere::db::{FileTaskTable, TaskTable};

    let parsed = WatchArgs::parse(args)?;
    let table = FileTaskTable::new(parsed.tasks_path.clone());
    let tools = LocalToolGateway;
    let interval = std::time::Duration::from_secs(10);

    log::info!(
        "[watch] Processed 태스크 폴링 시작 ({})",
        parsed.tasks_path.display()
    );
    loop {
        match table.fetch_processed().await {
            Ok(tasks) if !tasks.is_empty() => {
                for task in tasks {
                    let run_id = derive_run_identity(Some(&task.task_id), None)?;
                    let finalize_path = finalize_record_path(&parsed.state, &run_id);
                    if finalize_path.exists() {
                        log::info!(
                            "[watch] finalize record exists; skipping {} ({})",
                            task.task_id,
                            finalize_path.display()
                        );
                        continue;
                    }

                    log::info!("[watch] Processed 태스크 발견: {}", task.task_id);
                    let branch = format!("canopus/{run_id}");
                    match post_approval(
                        &parsed.repo,
                        &branch,
                        &run_id,
                        None,
                        &tools,
                        FinalizeMode::DryRun,
                    )
                    .await
                    .and_then(|output| {
                        persist_finalize_record_if_absent(&parsed.state, &run_id, &output)
                    }) {
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
            "CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION",
            "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION",
            "CANOPUS_ALLOW_GITHUB_REPO_CREATE",
            "CANOPUS_MOCK_GITHUB",
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
