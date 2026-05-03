use crate::adapters::agent_runtime::{CommandAgentRuntime, MockAgentRuntime};
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::github::{
    build_project_sync_plan, GitHubClient, GitHubProjectGates, GitHubProjectSyncConfig,
    GitHubProjectSyncReport, ProjectOwnerKind,
};
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::core::{
    derive_run_identity, Agenda, AgentRole, AgentTask, Artifact, ArtifactKind, CanopusError,
    CanopusResult, GitHubIssueMetadata, GitHubProjectMetadata, GitHubProjectMode, Pipeline,
    StageRecord, WorkflowState,
};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use chrono::{DateTime, Utc};
use dysonsphere::message::TaskType;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub async fn run(args: Vec<String>) -> CanopusResult<()> {
    if args.len() < 2 {
        return Err(CanopusError::InvalidInput(usage()));
    }

    match args[1].as_str() {
        "submit" => submit(&args[2..]).await,
        "watch" => watch(&args[2..]).await,
        "finalize" => finalize(&args[2..]).await,
        "status" => status(&args[2..]),
        "artifacts" => artifacts(&args[2..]),
        _ => Err(CanopusError::InvalidInput(usage())),
    }
}

macro_rules! try_stage {
    ($expr:expr, $stage:expr, $state_path:expr, $agenda_id:expr, $records:expr) => {
        match $expr {
            Ok(value) => value,
            Err(err) => {
                $records.push($stage.record("failed", Vec::new()));
                let _ = persist_run_records($state_path, $agenda_id, $records);
                return Err(err);
            }
        }
    };
}

async fn submit(args: &[String]) -> CanopusResult<()> {
    let parsed = SubmitArgs::parse(args)?;
    // Note: changed_files excludes .canopus/ paths; --state must resolve under .canopus/ for MVP.
    let run_id = derive_run_identity(parsed.agenda_id.as_deref(), parsed.task_id.as_deref())?;
    let agenda = Agenda::new_with_id(&run_id, parsed.request.clone())?;
    let branch = format!("canopus/{}", agenda.id);
    let artifact_store = LocalFileArtifactStore::new(parsed.state.join("artifacts"));
    let backend = StellarisTaskBackend::new(parsed.state.join("tasks.json"))?;
    let runtime = selected_runtime()?;
    let tools = LocalToolGateway;
    let mut stage_records: Vec<StageRecord> = Vec::new();

    let stage = StageTimer::start("prepare");
    try_stage!(
        tools.ensure_clean_worktree(&parsed.repo),
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    try_stage!(
        tools.create_branch(&parsed.repo, &branch),
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    stage_records.push(stage.finish("ok", vec![]));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;
    persist_upstream_provenance(
        &artifact_store,
        &parsed.state,
        &agenda.id,
        &parsed,
        &mut stage_records,
    )?;

    let pipeline = parsed
        .task_type
        .as_ref()
        .map(Pipeline::from_task_type)
        .unwrap_or(Pipeline::DevMode);
    log::info!(
        "[pipeline] selected {:?} for agenda {} (task_type={:?})",
        pipeline,
        agenda.id,
        parsed.task_type
    );

    let mut state = WorkflowState::Created;
    let mut prior_artifacts: Vec<Artifact> = Vec::new();
    let mut check_completed = false;
    let mut reviewed = false;
    let mut qa_issue_number = parsed.github_issue_number;

    for (index, role) in pipeline.agent_roles().into_iter().enumerate() {
        let stage_name = stage_name_for_role(&role);
        let stage = StageTimer::start(stage_name);
        let task_id = format!("TASK-{index}-{stage_name}");
        let task = agent_task(&agenda, &task_id, role.clone(), &parsed);

        try_stage!(
            backend.submit(&task).await,
            &stage,
            &parsed.state,
            &agenda.id,
            &mut stage_records
        );
        let result = try_stage!(
            runtime
                .run(
                    &task,
                    &AgentContext {
                        repo_path: parsed.repo.clone(),
                    },
                    &prior_artifacts,
                )
                .await,
            &stage,
            &parsed.state,
            &agenda.id,
            &mut stage_records
        );

        let mut stage_artifacts = Vec::new();
        for artifact in &result.artifacts {
            stage_artifacts.push(
                try_stage!(
                    artifact_store.save(artifact),
                    &stage,
                    &parsed.state,
                    &agenda.id,
                    &mut stage_records
                )
                .path
                .display()
                .to_string(),
            );
        }
        stage_records.push(stage.finish("ok", stage_artifacts));
        persist_run_records(&parsed.state, &agenda.id, &stage_records)?;

        if stage_name == "analyst" && parsed.allow_github_mutation {
            qa_issue_number = maybe_create_qa_issue(&agenda, &result.artifacts);
        } else if stage_name == "analyst" {
            log::info!("[analyst] GitHub mutation disabled — using metadata/dry-run path");
        }

        prior_artifacts.extend(result.artifacts.iter().cloned());

        if state == WorkflowState::Created {
            state = state
                .transition_to(WorkflowState::Planned)
                .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
            log::info!("[workflow] state: {:?}", state);
        }

        match role {
            AgentRole::Planner => {
                notify_discord("📋 **Plan 완료** — 플래너가 작업 계획을 수립했습니다.");
            }
            AgentRole::Coder => {
                notify_discord("💻 **Code 완료** — 코더가 변경사항을 적용했습니다.");
                if state == WorkflowState::Planned {
                    state = state
                        .transition_to(WorkflowState::Executing)
                        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
                    log::info!("[workflow] state: {:?}", state);
                }
                run_check_stage(
                    &tools,
                    &artifact_store,
                    &parsed.repo,
                    &parsed.state,
                    &agenda.id,
                    &task.id,
                    &mut stage_records,
                )?;
                check_completed = true;
                state = state
                    .transition_to(WorkflowState::Checking)
                    .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
                log::info!("[workflow] state: {:?}", state);
            }
            AgentRole::Reviewer => {
                notify_discord("🔍 **Review 완료** — 리뷰어 검토가 완료됐습니다.");
                if state == WorkflowState::Checking {
                    state = state
                        .transition_to(WorkflowState::Reviewed)
                        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
                    log::info!("[workflow] state: {:?}", state);
                    reviewed = true;
                }
            }
            AgentRole::Custom(_) => {}
        }
    }

    if !check_completed {
        if state == WorkflowState::Planned {
            state = state
                .transition_to(WorkflowState::Executing)
                .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
            log::info!("[workflow] state: {:?}", state);
        }
        let fallback_task_id = format!("{}-check", agenda.id);
        run_check_stage(
            &tools,
            &artifact_store,
            &parsed.repo,
            &parsed.state,
            &agenda.id,
            &fallback_task_id,
            &mut stage_records,
        )?;
        state = state
            .transition_to(WorkflowState::Checking)
            .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
        log::info!("[workflow] state: {:?}", state);
    }

    if !reviewed && state == WorkflowState::Checking {
        state = state
            .transition_to(WorkflowState::Reviewed)
            .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
        log::info!("[workflow] state: {:?}", state);
    }

    run_github_project_stage(
        &artifact_store,
        &parsed.state,
        &agenda.id,
        &parsed,
        &mut stage_records,
    )?;

    state = state
        .transition_to(WorkflowState::Completed)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);
    let stage = StageTimer::start("complete");
    stage_records.push(stage.finish(
        "ok",
        vec![parsed
            .state
            .join("runs")
            .join(format!("{}.json", agenda.id))
            .display()
            .to_string()],
    ));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;

    println!(
        "Canopus task {} completed local patch flow on branch {branch}",
        agenda.id
    );
    if let Some(n) = qa_issue_number {
        log::info!("[submit] Q&A issue #{n} — watch/finalize may close it after approval");
    }
    Ok(())
}

fn stage_name_for_role(role: &AgentRole) -> &str {
    match role {
        AgentRole::Planner => "plan",
        AgentRole::Coder => "code",
        AgentRole::Reviewer => "review",
        AgentRole::Custom(name) => name.as_str(),
    }
}

fn run_check_stage(
    tools: &LocalToolGateway,
    artifact_store: &LocalFileArtifactStore,
    repo: &Path,
    state_path: &Path,
    agenda_id: &str,
    task_id: &str,
    stage_records: &mut Vec<StageRecord>,
) -> CanopusResult<()> {
    let stage = StageTimer::start("check");
    let mut check_artifacts = Vec::new();
    let diff = try_stage!(
        tools.changed_files(repo),
        &stage,
        state_path,
        agenda_id,
        stage_records
    );
    let diff_artifact = Artifact {
        task_id: task_id.to_string(),
        kind: ArtifactKind::Diff,
        content: format!("# Diff\n\n```text\n{}```\n", diff.stdout),
    };
    check_artifacts.push(
        try_stage!(
            artifact_store.save(&diff_artifact),
            &stage,
            state_path,
            agenda_id,
            stage_records
        )
        .path
        .display()
        .to_string(),
    );

    let check = try_stage!(
        tools.run_check(repo, &["git", "diff", "--check"]),
        &stage,
        state_path,
        agenda_id,
        stage_records
    );
    let check_artifact = Artifact {
        task_id: task_id.to_string(),
        kind: ArtifactKind::TestResult,
        content: format!(
            "# Check\n\nstatus: {}\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
            check.status, check.stdout, check.stderr
        ),
    };
    check_artifacts.push(
        try_stage!(
            artifact_store.save(&check_artifact),
            &stage,
            state_path,
            agenda_id,
            stage_records
        )
        .path
        .display()
        .to_string(),
    );
    stage_records.push(stage.finish("ok", check_artifacts));
    persist_run_records(state_path, agenda_id, stage_records)
}

fn run_github_project_stage(
    artifact_store: &LocalFileArtifactStore,
    state_path: &Path,
    agenda_id: &str,
    parsed: &SubmitArgs,
    stage_records: &mut Vec<StageRecord>,
) -> CanopusResult<()> {
    if !github_project_sync_requested(parsed) {
        return Ok(());
    }

    let stage = StageTimer::start("github-project");
    let artifact = try_stage!(
        github_project_sync_artifact(agenda_id, parsed),
        &stage,
        state_path,
        agenda_id,
        stage_records
    );
    let saved = try_stage!(
        artifact_store.save(&artifact),
        &stage,
        state_path,
        agenda_id,
        stage_records
    );
    stage_records.push(stage.finish("ok", vec![saved.path.display().to_string()]));
    persist_run_records(state_path, agenda_id, stage_records)
}

fn github_project_sync_artifact(agenda_id: &str, parsed: &SubmitArgs) -> CanopusResult<Artifact> {
    let mode = parsed.github_project_mode;
    if parsed.github_project_item_id.is_none() && parsed.github_issue_number.is_none() {
        if matches!(mode, GitHubProjectMode::DryRunOffline) {
            return Ok(Artifact {
                task_id: agenda_id.to_string(),
                kind: ArtifactKind::RuntimeLog,
                content: format!(
                    "# GitHub Project v2 Sync\n\nmode: {}\nstatus: skipped\nreason: github_project_item_id or github_issue_number is required before a Project item can be added or updated\nhttp: none\nmutation: none\n",
                    mode.as_str()
                ),
            });
        }
        return Err(CanopusError::InvalidInput(
            "GitHub Project sync requires --github-project-item-id or --github-issue-number"
                .to_string(),
        ));
    }

    let config = github_project_sync_config(parsed)?;
    let gates = github_project_gates_from_env();
    match mode {
        GitHubProjectMode::DryRunOffline => {
            let plan = build_project_sync_plan(&config, &gates)?;
            Ok(Artifact {
                task_id: agenda_id.to_string(),
                kind: ArtifactKind::RuntimeLog,
                content: github_project_plan_markdown(&plan, &gates),
            })
        }
        GitHubProjectMode::ValidateReadOnly | GitHubProjectMode::MutateLive => {
            let _plan = build_project_sync_plan(&config, &gates)?;
            let client = GitHubClient::from_env().ok_or_else(|| {
                CanopusError::InvalidInput(
                    "GitHub Project validation/mutation requires GITHUB_TOKEN, GITHUB_OWNER, and GITHUB_REPO".to_string(),
                )
            })?;
            let report = client.sync_project_v2(&config, &gates)?;
            Ok(Artifact {
                task_id: agenda_id.to_string(),
                kind: ArtifactKind::RuntimeLog,
                content: github_project_report_markdown(&report, &gates),
            })
        }
    }
}

fn github_project_plan_markdown(
    plan: &crate::adapters::github::GitHubProjectSyncPlan,
    gates: &GitHubProjectGates,
) -> String {
    let operations = plan
        .operations
        .iter()
        .map(|operation| format!("- {:?}: {}", operation.kind, operation.operation_name))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# GitHub Project v2 Sync\n\nmode: {}\nhttp: none\nmutation: none\nproject_id_source: {}\nitem_id_source: {}\nstatus_field_source: {}\nstatus_option_source: {}\ngates: enable_github={}, enable_live_mutations={}, allow_project_mutation={}\n\n## Planned operations\n{}\n",
        plan.mode.as_str(),
        plan.project_id_source,
        plan.item_id_source,
        plan.status_field_source,
        plan.status_option_source,
        gates.enable_github,
        gates.enable_live_mutations,
        gates.allow_project_mutation,
        if operations.is_empty() {
            "- (none)".to_string()
        } else {
            operations
        }
    )
}

fn github_project_report_markdown(
    report: &GitHubProjectSyncReport,
    gates: &GitHubProjectGates,
) -> String {
    let executed = report
        .executed_operations
        .iter()
        .map(|operation| format!("- {operation}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# GitHub Project v2 Sync\n\nmode: {}\nproject_id: {}\nitem_id: {}\ngates: enable_github={}, enable_live_mutations={}, allow_project_mutation={}\n\n## Executed operations\n{}\n",
        report.mode.as_str(),
        report.project_id.as_deref().unwrap_or("(unresolved)"),
        report.item_id.as_deref().unwrap_or("(unresolved)"),
        gates.enable_github,
        gates.enable_live_mutations,
        gates.allow_project_mutation,
        if executed.is_empty() {
            "- (none)".to_string()
        } else {
            executed
        }
    )
}

fn maybe_create_qa_issue(agenda: &Agenda, artifacts: &[Artifact]) -> Option<u64> {
    let Some(gh) = GitHubClient::from_env() else {
        log::info!("[analyst] GitHub env missing — Q&A skipped");
        return None;
    };

    let questions = artifacts
        .iter()
        .map(|a| a.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let title = format!("[Canopus Q&A] {}", agenda.request);
    match gh.create_issue(&title, &questions) {
        Ok(n) => {
            notify_discord(&format!("❓ **Q&A Issue 생성** — #{n} 에 답변해 주세요."));
            Some(n)
        }
        Err(e) => {
            log::warn!("[analyst] GitHub Issue creation failed: {e}");
            None
        }
    }
}

async fn watch(args: &[String]) -> CanopusResult<()> {
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

async fn finalize(args: &[String]) -> CanopusResult<()> {
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
enum FinalizeMode {
    DryRun,
    Mutate,
}

async fn post_approval(
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

fn persist_upstream_provenance(
    artifact_store: &LocalFileArtifactStore,
    state_path: &Path,
    agenda_id: &str,
    parsed: &SubmitArgs,
    stage_records: &mut Vec<StageRecord>,
) -> CanopusResult<()> {
    let Some(content) = parsed.upstream_provenance_markdown() else {
        return Ok(());
    };

    let stage = StageTimer::start("upstream-provenance");
    let artifact = Artifact {
        task_id: format!("{agenda_id}-upstream-provenance"),
        kind: ArtifactKind::RuntimeLog,
        content,
    };
    let saved = artifact_store.save(&artifact)?;
    stage_records.push(stage.finish("ok", vec![saved.path.display().to_string()]));
    persist_run_records(state_path, agenda_id, stage_records)
}

fn live_mutations_enabled() -> bool {
    std::env::var("CANOPUS_ENABLE_LIVE_MUTATIONS").as_deref() == Ok("1")
}

fn status(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput(
            "usage: canopus status <task-id>".to_string(),
        ));
    }
    println!("{}: local status is file-backed in MVP", args[0]);
    Ok(())
}

fn artifacts(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput(
            "usage: canopus artifacts <task-id>".to_string(),
        ));
    }
    println!("artifacts for {} are under .canopus/artifacts", args[0]);
    Ok(())
}

struct StageTimer {
    name: String,
    started_at: DateTime<Utc>,
    instant: Instant,
}

impl StageTimer {
    fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            started_at: Utc::now(),
            instant: Instant::now(),
        }
    }

    fn finish(self, status: impl Into<String>, artifacts: Vec<String>) -> StageRecord {
        self.record(status, artifacts)
    }

    fn record(&self, status: impl Into<String>, artifacts: Vec<String>) -> StageRecord {
        let ended_at = Utc::now();
        StageRecord {
            name: self.name.clone(),
            started_at: self.started_at,
            ended_at,
            duration_secs: self.instant.elapsed().as_secs(),
            status: status.into(),
            artifacts,
        }
    }
}

fn persist_run_records(
    state: &Path,
    agenda_id: &str,
    records: &[StageRecord],
) -> CanopusResult<()> {
    let runs_dir = state.join("runs");
    fs::create_dir_all(&runs_dir)?;
    let path = runs_dir.join(format!("{agenda_id}.json"));
    let json = serde_json::to_vec_pretty(records)
        .map_err(|e| CanopusError::Runtime(format!("serialize stage records: {e}")))?;
    fs::write(path, json)?;
    Ok(())
}

struct SubmitArgs {
    repo: PathBuf,
    state: PathBuf,
    request: String,
    task_type: Option<TaskType>,
    agenda_id: Option<String>,
    task_id: Option<String>,
    task_status: Option<String>,
    task_created_at: Option<String>,
    task_updated_at: Option<String>,
    role_mode: String,
    github_owner: Option<String>,
    github_repo: Option<String>,
    github_issue_number: Option<u64>,
    github_issue_url: Option<String>,
    github_project_mode: GitHubProjectMode,
    github_project_mode_explicit: bool,
    github_project_id: Option<String>,
    github_project_url: Option<String>,
    github_project_item_id: Option<String>,
    github_project_status: Option<String>,
    github_project_owner_kind: Option<String>,
    github_project_owner: Option<String>,
    github_project_number: Option<u64>,
    github_project_status_field_id: Option<String>,
    github_project_status_field_name: Option<String>,
    github_project_status_option_id: Option<String>,
    github_project_status_option_name: Option<String>,
    allow_github_mutation: bool,
}

impl SubmitArgs {
    fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = PathBuf::from(".");
        let mut state = PathBuf::from(".canopus");
        let mut request_parts = Vec::new();
        let mut task_type = None;
        let mut agenda_id = None;
        let mut task_id = None;
        let mut task_status = None;
        let mut task_created_at = None;
        let mut task_updated_at = None;
        let mut role_mode = "standard".to_string();
        let mut github_owner = env_non_empty("GITHUB_OWNER");
        let mut github_repo = env_non_empty("GITHUB_REPO");
        let mut github_issue_number = None;
        let mut github_issue_url = None;
        let mut github_project_mode = match env_non_empty("CANOPUS_GITHUB_PROJECT_MODE").as_deref()
        {
            Some(value) => GitHubProjectMode::parse(value)?,
            None => GitHubProjectMode::DryRunOffline,
        };
        let mut github_project_mode_explicit =
            env_non_empty("CANOPUS_GITHUB_PROJECT_MODE").is_some();
        let mut github_project_id = env_non_empty("GITHUB_PROJECT_ID");
        let mut github_project_url = env_non_empty("GITHUB_PROJECT_URL");
        let mut github_project_item_id = env_non_empty("GITHUB_PROJECT_ITEM_ID");
        let mut github_project_status = env_non_empty("GITHUB_PROJECT_STATUS");
        let mut github_project_owner_kind = env_non_empty("GITHUB_PROJECT_OWNER_KIND");
        let mut github_project_owner = env_non_empty("GITHUB_PROJECT_OWNER");
        let mut github_project_number = env_u64("GITHUB_PROJECT_NUMBER")?;
        let mut github_project_status_field_id = env_non_empty("GITHUB_PROJECT_STATUS_FIELD_ID");
        let mut github_project_status_field_name =
            env_non_empty("GITHUB_PROJECT_STATUS_FIELD_NAME");
        let mut github_project_status_option_id = env_non_empty("GITHUB_PROJECT_STATUS_OPTION_ID");
        let mut github_project_status_option_name =
            env_non_empty("GITHUB_PROJECT_STATUS_OPTION_NAME");
        let mut request_github_mutation = env_flag("CANOPUS_ALLOW_GITHUB_MUTATION");
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = PathBuf::from(required_value(args, index, "--repo")?);
                }
                "--state" => {
                    index += 1;
                    state = PathBuf::from(required_value(args, index, "--state")?);
                }
                "--task-type" => {
                    index += 1;
                    task_type = Some(parse_task_type(required_value(
                        args,
                        index,
                        "--task-type",
                    )?)?);
                }
                "--agenda-id" => {
                    index += 1;
                    agenda_id = Some(required_value(args, index, "--agenda-id")?.to_string());
                }
                "--task-id" => {
                    index += 1;
                    task_id = Some(required_value(args, index, "--task-id")?.to_string());
                }
                "--task-status" => {
                    index += 1;
                    task_status = Some(required_value(args, index, "--task-status")?.to_string());
                }
                "--task-created-at" => {
                    index += 1;
                    task_created_at =
                        Some(required_value(args, index, "--task-created-at")?.to_string());
                }
                "--task-updated-at" => {
                    index += 1;
                    task_updated_at =
                        Some(required_value(args, index, "--task-updated-at")?.to_string());
                }
                "--role-mode" => {
                    index += 1;
                    role_mode = required_value(args, index, "--role-mode")?.to_string();
                }
                "--github-owner" => {
                    index += 1;
                    github_owner = Some(required_value(args, index, "--github-owner")?.to_string());
                }
                "--github-repo" => {
                    index += 1;
                    github_repo = Some(required_value(args, index, "--github-repo")?.to_string());
                }
                "--github-issue-number" => {
                    index += 1;
                    let value = required_value(args, index, "--github-issue-number")?;
                    github_issue_number = Some(value.parse().map_err(|_| {
                        CanopusError::InvalidInput(
                            "--github-issue-number must be a positive integer".to_string(),
                        )
                    })?);
                }
                "--github-issue-url" => {
                    index += 1;
                    github_issue_url =
                        Some(required_value(args, index, "--github-issue-url")?.to_string());
                }
                "--github-project-id" => {
                    index += 1;
                    github_project_id =
                        Some(required_value(args, index, "--github-project-id")?.to_string());
                }
                "--github-project-url" => {
                    index += 1;
                    github_project_url =
                        Some(required_value(args, index, "--github-project-url")?.to_string());
                }
                "--github-project-item-id" => {
                    index += 1;
                    github_project_item_id =
                        Some(required_value(args, index, "--github-project-item-id")?.to_string());
                }
                "--github-project-status" => {
                    index += 1;
                    github_project_status =
                        Some(required_value(args, index, "--github-project-status")?.to_string());
                }
                "--github-project-owner-kind" => {
                    index += 1;
                    github_project_owner_kind = Some(
                        required_value(args, index, "--github-project-owner-kind")?.to_string(),
                    );
                }
                "--github-project-owner" => {
                    index += 1;
                    github_project_owner =
                        Some(required_value(args, index, "--github-project-owner")?.to_string());
                }
                "--github-project-number" => {
                    index += 1;
                    let value = required_value(args, index, "--github-project-number")?;
                    github_project_number = Some(value.parse().map_err(|_| {
                        CanopusError::InvalidInput(
                            "--github-project-number must be a positive integer".to_string(),
                        )
                    })?);
                }
                "--github-project-status-field-id" => {
                    index += 1;
                    github_project_status_field_id = Some(
                        required_value(args, index, "--github-project-status-field-id")?
                            .to_string(),
                    );
                }
                "--github-project-status-field-name" => {
                    index += 1;
                    github_project_status_field_name = Some(
                        required_value(args, index, "--github-project-status-field-name")?
                            .to_string(),
                    );
                }
                "--github-project-status-option-id" => {
                    index += 1;
                    github_project_status_option_id = Some(
                        required_value(args, index, "--github-project-status-option-id")?
                            .to_string(),
                    );
                }
                "--github-project-status-option-name" => {
                    index += 1;
                    github_project_status_option_name = Some(
                        required_value(args, index, "--github-project-status-option-name")?
                            .to_string(),
                    );
                }
                "--github-project-mode" => {
                    index += 1;
                    github_project_mode = GitHubProjectMode::parse(required_value(
                        args,
                        index,
                        "--github-project-mode",
                    )?)?;
                    github_project_mode_explicit = true;
                }
                "--allow-github-mutation" => request_github_mutation = true,
                value if value.starts_with("--") => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown submit argument: {value}"
                    )));
                }
                value => request_parts.push(value.to_string()),
            }
            index += 1;
        }

        let request = request_parts.join(" ");
        if request.trim().is_empty() {
            return Err(CanopusError::InvalidInput(
                "submit requires a request".to_string(),
            ));
        }

        let allow_github_mutation = env_flag("CANOPUS_ENABLE_GITHUB") && request_github_mutation;

        Ok(Self {
            repo,
            state,
            request,
            task_type,
            agenda_id,
            task_id,
            task_status,
            task_created_at,
            task_updated_at,
            role_mode,
            github_owner,
            github_repo,
            github_issue_number,
            github_issue_url,
            github_project_mode,
            github_project_mode_explicit,
            github_project_id,
            github_project_url,
            github_project_item_id,
            github_project_status,
            github_project_owner_kind,
            github_project_owner,
            github_project_number,
            github_project_status_field_id,
            github_project_status_field_name,
            github_project_status_option_id,
            github_project_status_option_name,
            allow_github_mutation,
        })
    }
}

impl SubmitArgs {
    fn has_upstream_provenance(&self) -> bool {
        self.task_status.is_some()
            || self.task_created_at.is_some()
            || self.task_updated_at.is_some()
    }

    fn upstream_provenance_markdown(&self) -> Option<String> {
        self.has_upstream_provenance().then(|| {
            format!(
                "# Upstream Task Provenance\n\nsource_task_id: {}\ntask_status: {}\ntask_created_at: {}\ntask_updated_at: {}\n",
                self.task_id.as_deref().unwrap_or("(none)"),
                self.task_status.as_deref().unwrap_or("(none)"),
                self.task_created_at.as_deref().unwrap_or("(none)"),
                self.task_updated_at.as_deref().unwrap_or("(none)")
            )
        })
    }
}

struct WatchArgs {
    repo: PathBuf,
    state: PathBuf,
    tasks_path: PathBuf,
    once: bool,
}

impl WatchArgs {
    fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = env_non_empty("CANOPUS_REPO")
            .or_else(|| env_non_empty("CANOPUS_REPO_PATH"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut state: Option<PathBuf> = env_non_empty("CANOPUS_STATE")
            .or_else(|| env_non_empty("CANOPUS_STATE_PATH"))
            .map(PathBuf::from);
        let mut tasks_path: Option<PathBuf> = None;
        let mut once = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = PathBuf::from(required_value(args, index, "--repo")?);
                }
                "--state" => {
                    index += 1;
                    state = Some(PathBuf::from(required_value(args, index, "--state")?));
                }
                "--once" => once = true,
                value if value.starts_with("--") => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown watch argument: {value}"
                    )));
                }
                value => {
                    if tasks_path.is_some() {
                        return Err(CanopusError::InvalidInput(
                            "watch accepts at most one tasks path".to_string(),
                        ));
                    }
                    tasks_path = Some(PathBuf::from(value));
                }
            }
            index += 1;
        }

        let state = state.unwrap_or_else(|| repo.join(".canopus"));
        let tasks_path = tasks_path.unwrap_or_else(|| state.join("tasks.json"));
        Ok(Self {
            repo,
            state,
            tasks_path,
            once,
        })
    }
}

struct FinalizeArgs {
    repo: PathBuf,
    state: Option<PathBuf>,
    agenda_id: Option<String>,
    task_id: Option<String>,
    github_issue_number: Option<u64>,
    allow_mutation: bool,
}

impl FinalizeArgs {
    fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = PathBuf::from(".");
        let mut state = None;
        let mut agenda_id = None;
        let mut task_id = None;
        let mut github_issue_number = None;
        let mut allow_mutation = env_flag("CANOPUS_FINALIZE_MUTATION");
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = PathBuf::from(required_value(args, index, "--repo")?);
                }
                "--state" => {
                    index += 1;
                    state = Some(PathBuf::from(required_value(args, index, "--state")?));
                }
                "--agenda-id" => {
                    index += 1;
                    agenda_id = Some(required_value(args, index, "--agenda-id")?.to_string());
                }
                "--task-id" => {
                    index += 1;
                    task_id = Some(required_value(args, index, "--task-id")?.to_string());
                }
                "--github-issue-number" => {
                    index += 1;
                    let value = required_value(args, index, "--github-issue-number")?;
                    github_issue_number = Some(value.parse().map_err(|_| {
                        CanopusError::InvalidInput(
                            "--github-issue-number must be a positive integer".to_string(),
                        )
                    })?);
                }
                "--allow-mutation" => allow_mutation = true,
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown finalize argument: {value}"
                    )));
                }
            }
            index += 1;
        }

        if agenda_id.is_none() && task_id.is_none() {
            return Err(CanopusError::InvalidInput(
                "finalize requires --agenda-id or --task-id".to_string(),
            ));
        }

        Ok(Self {
            repo,
            state,
            agenda_id,
            task_id,
            github_issue_number,
            allow_mutation,
        })
    }
}

fn agent_task(agenda: &Agenda, suffix: &str, role: AgentRole, parsed: &SubmitArgs) -> AgentTask {
    let id = format!("{}-{}", agenda.id, suffix);
    let mut task = AgentTask::for_agenda_with_all_metadata(
        id,
        agenda,
        role,
        parsed.role_mode.clone(),
        parsed.task_id.clone(),
        github_issue_metadata(parsed),
        github_project_metadata(parsed),
    );
    if parsed.has_upstream_provenance() {
        task.prompt.push_str(&format!(
            "\nUpstream task status: {}",
            parsed.task_status.as_deref().unwrap_or("(none)")
        ));
        task.prompt.push_str(&format!(
            "\nUpstream task created_at: {}",
            parsed.task_created_at.as_deref().unwrap_or("(none)")
        ));
        task.prompt.push_str(&format!(
            "\nUpstream task updated_at: {}",
            parsed.task_updated_at.as_deref().unwrap_or("(none)")
        ));
    }
    task
}

fn github_issue_metadata(parsed: &SubmitArgs) -> Option<GitHubIssueMetadata> {
    if parsed.github_owner.is_none()
        && parsed.github_repo.is_none()
        && parsed.github_issue_number.is_none()
        && parsed.github_issue_url.is_none()
    {
        return None;
    }

    Some(GitHubIssueMetadata {
        owner: parsed.github_owner.clone(),
        repo: parsed.github_repo.clone(),
        number: parsed.github_issue_number,
        url: parsed.github_issue_url.clone(),
    })
}

fn github_project_metadata(parsed: &SubmitArgs) -> Option<GitHubProjectMetadata> {
    let metadata = GitHubProjectMetadata {
        id: parsed.github_project_id.clone(),
        url: parsed.github_project_url.clone(),
        item_id: parsed.github_project_item_id.clone(),
        status: parsed.github_project_status.clone(),
        owner_kind: parsed.github_project_owner_kind.clone(),
        owner: parsed.github_project_owner.clone(),
        number: parsed.github_project_number,
        status_field_id: parsed.github_project_status_field_id.clone(),
        status_field_name: parsed.github_project_status_field_name.clone(),
        status_option_id: parsed.github_project_status_option_id.clone(),
        status_option_name: parsed.github_project_status_option_name.clone(),
        mode: if parsed.github_project_mode_explicit {
            Some(parsed.github_project_mode)
        } else {
            None
        },
    };
    (!metadata.is_empty()).then_some(metadata)
}

fn github_project_sync_requested(parsed: &SubmitArgs) -> bool {
    parsed.github_project_id.is_some()
        || parsed.github_project_url.is_some()
        || parsed.github_project_item_id.is_some()
        || parsed.github_project_owner_kind.is_some()
        || parsed.github_project_owner.is_some()
        || parsed.github_project_number.is_some()
}

fn github_project_sync_config(parsed: &SubmitArgs) -> CanopusResult<GitHubProjectSyncConfig> {
    let project_owner_kind = match parsed.github_project_owner_kind.as_deref() {
        Some(value) if !value.trim().is_empty() => Some(ProjectOwnerKind::parse(value)?),
        _ => None,
    };

    Ok(GitHubProjectSyncConfig {
        mode: parsed.github_project_mode,
        project_id: parsed.github_project_id.clone(),
        project_url: parsed.github_project_url.clone(),
        project_owner_kind,
        project_owner: parsed.github_project_owner.clone(),
        project_number: parsed.github_project_number,
        repo_owner: parsed.github_owner.clone(),
        repo_name: parsed.github_repo.clone(),
        issue_number: parsed.github_issue_number,
        project_item_id: parsed.github_project_item_id.clone(),
        status_field_id: parsed.github_project_status_field_id.clone(),
        status_field_name: parsed.github_project_status_field_name.clone(),
        status_option_id: parsed.github_project_status_option_id.clone(),
        status_option_name: parsed.github_project_status_option_name.clone(),
        status: parsed.github_project_status.clone(),
    })
}

fn github_project_gates_from_env() -> GitHubProjectGates {
    GitHubProjectGates {
        enable_github: env_flag("CANOPUS_ENABLE_GITHUB"),
        enable_live_mutations: env_flag("CANOPUS_ENABLE_LIVE_MUTATIONS"),
        allow_project_mutation: env_flag("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION"),
    }
}

fn selected_runtime() -> CanopusResult<Box<dyn AgentRuntime>> {
    match std::env::var("CANOPUS_AGENT_RUNTIME").as_deref() {
        Ok("command") => Ok(Box::new(CommandAgentRuntime::from_env().ok_or_else(
            || {
                CanopusError::InvalidInput(
                    "CANOPUS_AGENT_RUNTIME=command requires CANOPUS_AGENT_COMMAND".to_string(),
                )
            },
        )?)),
        Ok("mock") | Err(_) => Ok(Box::new(MockAgentRuntime)),
        Ok(value) => Err(CanopusError::InvalidInput(format!(
            "unsupported CANOPUS_AGENT_RUNTIME: {value}"
        ))),
    }
}

fn required_value<'a>(args: &'a [String], index: usize, name: &str) -> CanopusResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CanopusError::InvalidInput(format!("{name} requires a value")))
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_u64(name: &str) -> CanopusResult<Option<u64>> {
    env_non_empty(name)
        .map(|value| {
            value.parse::<u64>().map_err(|_| {
                CanopusError::InvalidInput(format!(
                    "{name} must be a positive integer when configured"
                ))
            })
        })
        .transpose()
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        let _ = ureq::post(&url).send_json(body);
    }
}

fn parse_task_type(value: &str) -> CanopusResult<TaskType> {
    match value {
        "news-a" | "newsa" => Ok(TaskType::NewsA),
        "bug" => Ok(TaskType::Bug),
        "security" => Ok(TaskType::Security),
        "test-coverage" | "testcoverage" => Ok(TaskType::TestCoverage),
        "ux-improvement" | "uximprovement" => Ok(TaskType::UXImprovement),
        custom if custom.starts_with("custom:") => Ok(TaskType::Custom(
            custom.trim_start_matches("custom:").to_string(),
        )),
        other => Err(CanopusError::InvalidInput(format!(
            "unsupported --task-type {other}"
        ))),
    }
}

fn usage() -> String {
    "usage: canopus submit [--repo <path>] [--state <path>] [--agenda-id <id>] [--task-type <type>] <request> | canopus watch [--repo <path>] [--state <path>] [--once] [tasks-path] | canopus finalize [--repo <path>] [--state <path>] (--agenda-id <id>|--task-id <id>)".to_string()
}

#[cfg(test)]
mod post_approval_tests {
    use super::*;
    use std::process::Command;

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
    async fn post_approval_uses_dry_run_for_external_mutations_by_default() {
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
}
