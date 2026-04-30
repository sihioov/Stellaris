use crate::adapters::agent_runtime::MockAgentRuntime;
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::github::GitHubClient;
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::core::{
    Agenda, AgentRole, AgentTask, Artifact, ArtifactKind, CanopusError, CanopusResult, Pipeline,
    StageRecord, WorkflowState,
};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use chrono::{DateTime, Utc};
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
    let agenda = Agenda::new_with_id("CANOPUS-1", parsed.request)?;
    let branch = format!("canopus/{}", agenda.id);
    let artifact_store = LocalFileArtifactStore::new(parsed.state.join("artifacts"));
    let backend = StellarisTaskBackend::new(parsed.state.join("tasks.json"))?;
    let runtime = MockAgentRuntime;
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

    let mut state = WorkflowState::Created;

    let pipeline = Pipeline::DevMode; // CLI에서는 항상 DevMode; 유지보수는 hubble이 task_type으로 결정
    log::info!("[pipeline] 선택된 파이프라인: {:?}", pipeline);

    let stage = StageTimer::start("analyst");
    let analyst_task = AgentTask::for_agenda(
        "TASK-0-analyst",
        &agenda,
        AgentRole::Custom("analyst".to_string()),
    );
    try_stage!(
        backend.submit(&analyst_task).await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let analyst_result = try_stage!(
        runtime
            .run(
                &analyst_task,
                &AgentContext {
                    repo_path: parsed.repo.clone(),
                },
                &[],
            )
            .await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    stage_records.push(stage.finish("ok", vec![]));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;

    // GitHub Q&A Issue 생성 (GITHUB_TOKEN 있을 때만)
    let qa_issue_number: Option<u64> = if let Some(gh) = GitHubClient::from_env() {
        let questions = analyst_result
            .artifacts
            .iter()
            .map(|a| a.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let title = format!("[Canopus Q&A] {}", agenda.request);
        match gh.create_issue(&title, &questions) {
            Ok(n) => {
                notify_discord(&format!("❓ **Q&A Issue 생성** — #{n} 에 답변해 주세요."));
                // 30초 간격으로 새 comment 감지될 때까지 폴링
                let mut known_count = 0usize;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    match gh.get_issue_comments(n) {
                        Ok(comments) if comments.len() > known_count => {
                            log::info!("[analyst] 답변 감지 — Planner 진행");
                            break;
                        }
                        Ok(comments) => known_count = comments.len(),
                        Err(e) => {
                            log::warn!("[analyst] comment 폴링 실패: {e}");
                            break; // 오류 시 건너뜀
                        }
                    }
                }
                Some(n)
            }
            Err(e) => {
                log::warn!("[analyst] GitHub Issue 생성 실패: {e}");
                None
            }
        }
    } else {
        log::info!("[analyst] GITHUB_TOKEN 없음 — Q&A 건너뜀");
        None
    };

    state = state
        .transition_to(WorkflowState::Planned)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

    let stage = StageTimer::start("plan");
    let plan_task = AgentTask::for_agenda("TASK-1-plan", &agenda, AgentRole::Planner);
    try_stage!(
        backend.submit(&plan_task).await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let plan_result = try_stage!(
        runtime
            .run(
                &plan_task,
                &AgentContext {
                    repo_path: parsed.repo.clone(),
                },
                &[],
            )
            .await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let mut plan_artifacts = Vec::new();
    for artifact in &plan_result.artifacts {
        plan_artifacts.push(
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
    stage_records.push(stage.finish("ok", plan_artifacts));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;
    notify_discord("📋 **Plan 완료** — 플래너가 작업 계획을 수립했습니다.");

    state = state
        .transition_to(WorkflowState::Executing)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

    let stage = StageTimer::start("code");
    let code_task = AgentTask::for_agenda("TASK-2-code", &agenda, AgentRole::Coder);
    try_stage!(
        backend.submit(&code_task).await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let code_result = try_stage!(
        runtime
            .run(
                &code_task,
                &AgentContext {
                    repo_path: parsed.repo.clone(),
                },
                &plan_result.artifacts,
            )
            .await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let mut code_artifacts = Vec::new();
    for artifact in &code_result.artifacts {
        code_artifacts.push(
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
    stage_records.push(stage.finish("ok", code_artifacts));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;
    notify_discord("💻 **Code 완료** — 코더가 변경사항을 적용했습니다.");

    state = state
        .transition_to(WorkflowState::Checking)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

    let stage = StageTimer::start("check");
    let mut check_artifacts = Vec::new();
    let diff = try_stage!(
        tools.changed_files(&parsed.repo),
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let diff_artifact = Artifact {
        task_id: code_task.id.clone(),
        kind: ArtifactKind::Diff,
        content: format!("# Diff\n\n```text\n{}```\n", diff.stdout),
    };
    check_artifacts.push(
        try_stage!(
            artifact_store.save(&diff_artifact),
            &stage,
            &parsed.state,
            &agenda.id,
            &mut stage_records
        )
        .path
        .display()
        .to_string(),
    );

    let check = try_stage!(
        tools.run_check(&parsed.repo, &["git", "diff", "--check"]),
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let check_artifact = Artifact {
        task_id: code_task.id.clone(),
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
            &parsed.state,
            &agenda.id,
            &mut stage_records
        )
        .path
        .display()
        .to_string(),
    );
    stage_records.push(stage.finish("ok", check_artifacts));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;

    let stage = StageTimer::start("review");
    let review_task = AgentTask::for_agenda("TASK-3-review", &agenda, AgentRole::Reviewer);
    try_stage!(
        backend.submit(&review_task).await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let mut review_prior: Vec<Artifact> = plan_result.artifacts.clone();
    review_prior.extend(code_result.artifacts.iter().cloned());
    let review_result = try_stage!(
        runtime
            .run(
                &review_task,
                &AgentContext {
                    repo_path: parsed.repo.clone(),
                },
                &review_prior,
            )
            .await,
        &stage,
        &parsed.state,
        &agenda.id,
        &mut stage_records
    );
    let mut review_artifacts = Vec::new();
    for artifact in &review_result.artifacts {
        review_artifacts.push(
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
    stage_records.push(stage.finish("ok", review_artifacts));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;
    state = state
        .transition_to(WorkflowState::Reviewed)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);
    notify_discord("🔍 **Review 완료** — 리뷰어 검토가 완료됐습니다.");
    state = state
        .transition_to(WorkflowState::Completed)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);
    let stage = StageTimer::start("complete");
    stage_records.push(stage.finish(
        "ok",
        vec![parsed.state.join("runs").join(format!("{}.json", agenda.id)).display().to_string()],
    ));
    persist_run_records(&parsed.state, &agenda.id, &stage_records)?;

    println!(
        "Canopus task {} completed local patch flow on branch {branch}",
        agenda.id
    );
    if let Some(n) = qa_issue_number {
        log::info!("[submit] Q&A issue #{n} — watch 명령으로 post_approval 시 close 예정");
    }
    Ok(())
}

async fn watch(args: &[String]) -> CanopusResult<()> {
    use dysonsphere::db::{FileTaskTable, TaskTable};

    let tasks_path = args.first().map(String::as_str).unwrap_or("tasks.json");
    let repo = std::env::var("CANOPUS_REPO").unwrap_or_else(|_| ".".to_string());
    let repo = std::path::Path::new(&repo);
    let table = FileTaskTable::new(std::path::PathBuf::from(tasks_path));
    let tools = LocalToolGateway;
    let interval = std::time::Duration::from_secs(10);

    log::info!("[watch] Processed 태스크 폴링 시작 ({})", tasks_path);
    loop {
        match table.fetch_processed().await {
            Ok(tasks) if !tasks.is_empty() => {
                for task in tasks {
                    log::info!("[watch] Processed 태스크 발견: {}", task.task_id);
                    let branch = format!("canopus/{}", task.task_id);
                    if let Err(e) = post_approval(repo, &branch, &task.task_id, None, &tools).await
                    {
                        log::error!("[watch] post_approval 실패 {}: {e}", task.task_id);
                    }
                }
            }
            Ok(_) => {}
            Err(e) => log::warn!("[watch] fetch_processed 실패: {e}"),
        }
        tokio::time::sleep(interval).await;
    }
}

async fn post_approval(
    repo: &std::path::Path,
    branch: &str,
    agenda_id: &str,
    qa_issue_number: Option<u64>,
    tools: &LocalToolGateway,
) -> CanopusResult<()> {
    tools.run_check(repo, &["git", "add", "-A"])?;
    let msg = format!("canopus: complete agenda {}", agenda_id);
    tools.run_check(repo, &["git", "commit", "-m", &msg])?;
    tools.run_check(repo, &["git", "push", "-u", "origin", branch])?;
    let pr_title = format!("[Canopus] {}", agenda_id);
    let pr_body = format!("Automated PR for agenda `{}`", agenda_id);
    tools.run_check(
        repo,
        &[
            "gh", "pr", "create", "--title", &pr_title, "--body", &pr_body, "--base", "main",
        ],
    )?;

    if let Some(issue_n) = qa_issue_number {
        if let Some(gh) = GitHubClient::from_env() {
            let _ = gh.close_issue(issue_n);
        }
    }

    notify_discord("🚀 **PR 생성 완료** — GitHub에서 Approve 후 merge 해주세요.");
    Ok(())
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
}

impl SubmitArgs {
    fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = PathBuf::from(".");
        let mut state = PathBuf::from(".canopus");
        let mut request_parts = Vec::new();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| {
                        CanopusError::InvalidInput("--repo requires a path".to_string())
                    })?;
                    repo = PathBuf::from(value);
                }
                "--state" => {
                    index += 1;
                    let value = args.get(index).ok_or_else(|| {
                        CanopusError::InvalidInput("--state requires a path".to_string())
                    })?;
                    state = PathBuf::from(value);
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

        Ok(Self {
            repo,
            state,
            request,
        })
    }
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        let _ = ureq::post(&url).send_json(body);
    }
}

fn usage() -> String {
    "usage: canopus submit [--repo <path>] [--state <path>] <request>".to_string()
}
