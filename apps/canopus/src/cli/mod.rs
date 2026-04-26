use crate::adapters::agent_runtime::MockAgentRuntime;
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::github::GitHubClient;
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::core::{
    Agenda, AgentRole, AgentTask, Artifact, ArtifactKind, CanopusError, CanopusResult,
    Pipeline, WorkflowState,
};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use std::path::PathBuf;

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

async fn submit(args: &[String]) -> CanopusResult<()> {
    let parsed = SubmitArgs::parse(args)?;
    // Note: changed_files excludes .canopus/ paths; --state must resolve under .canopus/ for MVP.
    let agenda = Agenda::new_with_id("CANOPUS-1", parsed.request)?;
    let branch = format!("canopus/{}", agenda.id);
    let artifact_store = LocalFileArtifactStore::new(parsed.state.join("artifacts"));
    let backend = StellarisTaskBackend::new(parsed.state.join("tasks.json"))?;
    let runtime = MockAgentRuntime;
    let tools = LocalToolGateway;

    tools.ensure_clean_worktree(&parsed.repo)?;
    tools.create_branch(&parsed.repo, &branch)?;

    let mut state = WorkflowState::Created;

    let pipeline = Pipeline::DevMode; // CLI에서는 항상 DevMode; 유지보수는 hubble이 task_type으로 결정
    log::info!("[pipeline] 선택된 파이프라인: {:?}", pipeline);

    let analyst_task = AgentTask::for_agenda("TASK-0-analyst", &agenda, AgentRole::Custom("analyst".to_string()));
    backend.submit(&analyst_task).await?;
    let analyst_result = runtime
        .run(
            &analyst_task,
            &AgentContext {
                repo_path: parsed.repo.clone(),
            },
            &[],
        )
        .await?;

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

    let plan_task = AgentTask::for_agenda("TASK-1-plan", &agenda, AgentRole::Planner);
    backend.submit(&plan_task).await?;
    let plan_result = runtime
        .run(
            &plan_task,
            &AgentContext {
                repo_path: parsed.repo.clone(),
            },
            &[],
        )
        .await?;
    for artifact in &plan_result.artifacts {
        artifact_store.save(artifact)?;
    }
    notify_discord("📋 **Plan 완료** — 플래너가 작업 계획을 수립했습니다.");

    state = state
        .transition_to(WorkflowState::Executing)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

    let code_task = AgentTask::for_agenda("TASK-2-code", &agenda, AgentRole::Coder);
    backend.submit(&code_task).await?;
    let code_result = runtime
        .run(
            &code_task,
            &AgentContext {
                repo_path: parsed.repo.clone(),
            },
            &plan_result.artifacts,
        )
        .await?;
    for artifact in &code_result.artifacts {
        artifact_store.save(artifact)?;
    }
    notify_discord("💻 **Code 완료** — 코더가 변경사항을 적용했습니다.");

    state = state
        .transition_to(WorkflowState::Checking)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

    let diff = tools.changed_files(&parsed.repo)?;
    artifact_store.save(&Artifact {
        task_id: code_task.id.clone(),
        kind: ArtifactKind::Diff,
        content: format!("# Diff\n\n```text\n{}```\n", diff.stdout),
    })?;

    let check = tools.run_check(&parsed.repo, &["git", "diff", "--check"])?;
    artifact_store.save(&Artifact {
        task_id: code_task.id.clone(),
        kind: ArtifactKind::TestResult,
        content: format!(
            "# Check\n\nstatus: {}\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
            check.status, check.stdout, check.stderr
        ),
    })?;

    let review_task = AgentTask::for_agenda("TASK-3-review", &agenda, AgentRole::Reviewer);
    backend.submit(&review_task).await?;
    let mut review_prior: Vec<Artifact> = plan_result.artifacts.clone();
    review_prior.extend(code_result.artifacts.iter().cloned());
    let review_result = runtime
        .run(
            &review_task,
            &AgentContext {
                repo_path: parsed.repo.clone(),
            },
            &review_prior,
        )
        .await?;
    for artifact in &review_result.artifacts {
        artifact_store.save(artifact)?;
    }
    state = state
        .transition_to(WorkflowState::Reviewed)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);
    notify_discord("🔍 **Review 완료** — 리뷰어 검토가 완료됐습니다.");
    state = state
        .transition_to(WorkflowState::Completed)
        .map_err(|e| CanopusError::InvalidInput(e.to_string()))?;
    log::info!("[workflow] state: {:?}", state);

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
                    if let Err(e) =
                        post_approval(repo, &branch, &task.task_id, None, &tools).await
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
            "gh",
            "pr",
            "create",
            "--title",
            &pr_title,
            "--body",
            &pr_body,
            "--base",
            "main",
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
