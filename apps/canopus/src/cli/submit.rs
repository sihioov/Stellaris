use crate::adapters::agent_runtime::{CommandAgentRuntime, MockAgentRuntime};
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::github::{
    build_project_sync_plan, GitHubClient, GitHubProjectGates, GitHubProjectSyncConfig,
    GitHubProjectSyncReport, ProjectOwnerKind,
};
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::cli::args::{env_flag, SubmitArgs};
use crate::cli::finalize::notify_discord;
use crate::core::{
    derive_run_identity, Agenda, AgentRole, AgentTask, Artifact, ArtifactKind, CanopusError,
    CanopusResult, GitHubIssueMetadata, GitHubProjectMetadata, GitHubProjectMode, Pipeline,
    StageRecord, WorkflowState,
};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;
use std::time::Instant;

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

pub(crate) async fn submit(args: &[String]) -> CanopusResult<()> {
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
