use crate::adapters::agent_runtime::MockAgentRuntime;
use crate::adapters::artifact_store::LocalFileArtifactStore;
use crate::adapters::task_backend::StellarisTaskBackend;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::core::{AgentRole, AgentTask, Agenda, Artifact, ArtifactKind, CanopusError, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime, ArtifactStore, TaskBackend, ToolGateway};
use std::path::PathBuf;

pub fn run(args: Vec<String>) -> CanopusResult<()> {
    if args.len() < 2 {
        return Err(CanopusError::InvalidInput(usage()));
    }

    match args[1].as_str() {
        "submit" => submit(&args[2..]),
        "status" => status(&args[2..]),
        "artifacts" => artifacts(&args[2..]),
        _ => Err(CanopusError::InvalidInput(usage())),
    }
}

fn submit(args: &[String]) -> CanopusResult<()> {
    let parsed = SubmitArgs::parse(args)?;
    std::fs::create_dir_all(&parsed.state)?;
    let agenda = Agenda::new_with_id("CANOPUS-1", parsed.request)?;
    let branch = format!("canopus/{}", agenda.id);
    let artifact_store = LocalFileArtifactStore::new(parsed.state.join("artifacts"));
    let backend = StellarisTaskBackend::new(parsed.state.join("tasks.json"))?;
    let runtime = MockAgentRuntime;
    let tools = LocalToolGateway;

    tools.ensure_clean_worktree(&parsed.repo)?;
    tools.create_branch(&parsed.repo, &branch)?;

    let plan_task = AgentTask::for_agenda("TASK-1-plan", &agenda, AgentRole::Planner);
    backend.submit(&plan_task)?;
    let plan_result = runtime.run(&plan_task, &AgentContext { repo_path: parsed.repo.clone() })?;
    for artifact in &plan_result.artifacts {
        artifact_store.save(artifact)?;
    }

    let code_task = AgentTask::for_agenda("TASK-2-code", &agenda, AgentRole::Coder);
    backend.submit(&code_task)?;
    let code_result = runtime.run(&code_task, &AgentContext { repo_path: parsed.repo.clone() })?;
    for artifact in &code_result.artifacts {
        artifact_store.save(artifact)?;
    }

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
    backend.submit(&review_task)?;
    let review_result = runtime.run(&review_task, &AgentContext { repo_path: parsed.repo })?;
    for artifact in &review_result.artifacts {
        artifact_store.save(artifact)?;
    }

    println!("Canopus task {} completed local patch flow on branch {branch}", agenda.id);
    Ok(())
}

fn status(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput("usage: canopus status <task-id>".to_string()));
    }
    println!("{}: local status is file-backed in MVP", args[0]);
    Ok(())
}

fn artifacts(args: &[String]) -> CanopusResult<()> {
    if args.len() != 1 {
        return Err(CanopusError::InvalidInput("usage: canopus artifacts <task-id>".to_string()));
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
            return Err(CanopusError::InvalidInput("submit requires a request".to_string()));
        }

        Ok(Self { repo, state, request })
    }
}

fn usage() -> String {
    "usage: canopus submit [--repo <path>] [--state <path>] <request>".to_string()
}
