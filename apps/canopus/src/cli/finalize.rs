use crate::adapters::github::GitHubClient;
use crate::adapters::tool_gateway::LocalToolGateway;
use crate::cli::args::{
    derive_state_with_source, env_flag, FinalizeApprovedArgs, FinalizeArgs, WatchArgs,
};
use crate::cli::commands::delivery_finalize::DeliveryGateReport;
use crate::core::commit_message::format_commit_message;
use crate::core::module_derivation::derive_modules;
use crate::core::{derive_run_identity, CanopusError, CanopusResult};
use crate::ports::ToolGateway;
use serde::Serialize;
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
                    let run_id = task.run_id.clone();
                    let (task_state, state_source) =
                        derive_state_with_source(task.repo_path.as_deref(), &parsed.state);
                    log::info!(
                        "[watch] finalize sidecar persist: {} (source={})",
                        task_state.display(),
                        state_source.as_str()
                    );
                    let finalize_path = finalize_record_path(&task_state, &run_id);
                    let mode = if env_flag("CANOPUS_ALLOW_LOCAL_COMMIT") {
                        FinalizeMode::LocalCommitOnly
                    } else {
                        FinalizeMode::DryRun
                    };
                    // PR-B migration-window idempotency guard (plan §5.1 / §5.4):
                    // when payload-derived state diverges from the watch-side
                    // --state argument, a previous release may have persisted
                    // the finalize sidecar at the legacy location. Honor it so
                    // we never re-finalize during the migration window. Remove
                    // this fallback one release after PR-B lands.
                    let legacy_finalize_path = (parsed.state != task_state)
                        .then(|| finalize_record_path(&parsed.state, &run_id));
                    if should_skip_existing_finalize_record(&finalize_path, mode) {
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
                    if let Some(legacy) = legacy_finalize_path
                        .as_ref()
                        .filter(|path| should_skip_existing_finalize_record(path, mode))
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
                    match post_approval(repo, &branch, &run_id, None, &tools, mode)
                        .await
                        .and_then(|output| {
                            persist_finalize_record_for_mode(&task_state, &run_id, &output, mode)
                        })
                        .and_then(|()| persist_delivery_gate_report_if_absent(&task_state, &run_id))
                    {
                        Ok(()) => {
                            log::info!(
                                "[watch] finalize record persisted for {} ({})",
                                task.task_id,
                                finalize_path.display()
                            );
                            notify_discord_token_usage(&task_state, &run_id);
                        }
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
    agenda_id: String,
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
        let agenda_id = payload
            .get("agenda_id")
            .or_else(|| payload.get("canopus_agenda_id"))
            .or_else(|| payload.get("run_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(task_id)
            .to_string();
        let run_id = derive_approved_run_id(&payload, &agenda_id, task_id)?;
        let repo_path = payload
            .get("repo_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        approved.push(ApprovedProcessedTask {
            task_id: task_id.to_string(),
            agenda_id,
            run_id,
            repo_path,
        });
    }
    Ok(approved)
}

fn derive_approved_run_id(
    payload: &serde_json::Map<String, Value>,
    agenda_id: &str,
    task_id: &str,
) -> CanopusResult<String> {
    if let Some(recorded_run_id) = payload
        .get("run_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return derive_run_identity(Some(recorded_run_id), None);
    }
    derive_run_identity(Some(agenda_id), Some(task_id))
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

pub(crate) async fn finalize_approved(args: &[String]) -> CanopusResult<()> {
    let parsed = match FinalizeApprovedArgs::parse(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            if args.iter().any(|arg| arg == "--json") {
                let response = FinalizeApprovedResponse::failure(
                    None,
                    "invalid_arguments",
                    err.to_string(),
                    false,
                    serde_json::json!({}),
                );
                print_finalize_approved_json(&response)?;
            }
            return Err(err);
        }
    };

    let result = finalize_approved_inner(&parsed).await;
    match result {
        Ok(response) => {
            if parsed.json {
                print_finalize_approved_json(&response)?;
            } else {
                println!(
                    "finalize-approved: {} {}",
                    response.task_id.as_deref().unwrap_or("(unknown)"),
                    response.status
                );
            }
            Ok(())
        }
        Err(error) => {
            if parsed.json {
                print_finalize_approved_json(&error.response)?;
            }
            Err(CanopusError::Runtime(error.message))
        }
    }
}

async fn finalize_approved_inner(
    parsed: &FinalizeApprovedArgs,
) -> Result<FinalizeApprovedResponse, FinalizeApprovedFailure> {
    let task = select_approved_task(&parsed.tasks_path, &parsed.task_id)?;
    let repo = task
        .repo_path
        .clone()
        .or_else(|| parsed.repo.clone())
        .ok_or_else(|| {
            finalize_failure(
                Some(&task),
                "repo_path_missing",
                "approved task payload does not include repo_path and --repo was not provided",
                true,
                serde_json::json!({ "tasks_path": parsed.tasks_path }),
            )
        })?;
    let parsed_state = parsed
        .state
        .clone()
        .unwrap_or_else(|| repo.join(".canopus"));
    let (task_state, _) = derive_state_with_source(Some(&repo), &parsed_state);
    let mode = if env_flag("CANOPUS_ALLOW_LOCAL_COMMIT") {
        FinalizeMode::LocalCommitOnly
    } else {
        FinalizeMode::DryRun
    };
    let branch = format!("canopus/{}", task.run_id);
    let sidecar_path = finalize_record_path(&task_state, &task.run_id);

    if should_skip_existing_finalize_record_for_branch(&sidecar_path, mode, &branch) {
        let commit = read_commit_from_record(&sidecar_path);
        let status = if matches!(mode, FinalizeMode::DryRun) {
            "dry_run"
        } else {
            "already_finalized"
        };
        return Ok(FinalizeApprovedResponse::success(
            &task,
            status,
            mode,
            &repo,
            &branch,
            commit,
            &sidecar_path,
            true,
            vec![],
        ));
    }

    let tools = LocalToolGateway;
    let output = post_approval(&repo, &branch, &task.run_id, None, &tools, mode)
        .await
        .map_err(|err| {
            finalize_failure(
                Some(&task),
                classify_finalize_error(&err),
                err.to_string(),
                true,
                serde_json::json!({ "repo_path": repo, "branch": branch }),
            )
        })?;
    persist_finalize_record_for_mode(&task_state, &task.run_id, &output, mode).map_err(|err| {
        finalize_failure(
            Some(&task),
            "sidecar_write_failed",
            err.to_string(),
            true,
            serde_json::json!({ "sidecar_path": sidecar_path }),
        )
    })?;
    persist_delivery_gate_report_if_absent(&task_state, &task.run_id).map_err(|err| {
        finalize_failure(
            Some(&task),
            "sidecar_write_failed",
            err.to_string(),
            true,
            serde_json::json!({ "sidecar_path": delivery_gate_record_path(&task_state, &task.run_id) }),
        )
    })?;

    let status = if matches!(mode, FinalizeMode::DryRun) {
        "dry_run"
    } else if output.contains("no changes") {
        "no_changes"
    } else {
        "finalized"
    };
    let commit = if matches!(mode, FinalizeMode::LocalCommitOnly) && status == "finalized" {
        current_commit(&tools, &repo)
    } else {
        None
    };
    Ok(FinalizeApprovedResponse::success(
        &task,
        status,
        mode,
        &repo,
        &branch,
        commit,
        &sidecar_path,
        false,
        vec![],
    ))
}

#[allow(clippy::result_large_err)]
fn select_approved_task(
    tasks_path: &Path,
    task_id: &str,
) -> Result<ApprovedProcessedTask, FinalizeApprovedFailure> {
    let text = fs::read_to_string(tasks_path).map_err(|err| {
        finalize_failure(
            None,
            "task_not_found",
            format!("cannot read tasks JSON: {err}"),
            true,
            serde_json::json!({ "tasks_path": tasks_path }),
        )
    })?;
    let value = serde_json::from_str::<Value>(&text).map_err(|err| {
        finalize_failure(
            None,
            "invalid_tasks_json",
            format!("invalid tasks JSON: {err}"),
            false,
            serde_json::json!({ "tasks_path": tasks_path }),
        )
    })?;
    let tasks = value.as_array().ok_or_else(|| {
        finalize_failure(
            None,
            "invalid_tasks_json",
            "tasks JSON must be an array",
            false,
            serde_json::json!({ "tasks_path": tasks_path }),
        )
    })?;

    let matches: Vec<&Value> = tasks
        .iter()
        .filter(|task| task.get("task_id").and_then(Value::as_str) == Some(task_id))
        .collect();
    if matches.is_empty() {
        return Err(finalize_failure(
            None,
            "task_not_found",
            format!("Task `{task_id}` was not found"),
            true,
            serde_json::json!({ "task_id": task_id }),
        ));
    }
    if matches.len() > 1 {
        return Err(finalize_failure(
            None,
            "ambiguous_task_id",
            format!("Task id `{task_id}` appears more than once"),
            false,
            serde_json::json!({ "task_id": task_id, "count": matches.len() }),
        ));
    }

    approved_processed_task_from_value(matches[0]).map_err(|(code, message, retryable)| {
        finalize_failure(
            None,
            code,
            message,
            retryable,
            serde_json::json!({ "task_id": task_id }),
        )
    })
}

fn approved_processed_task_from_value(
    task: &Value,
) -> Result<ApprovedProcessedTask, (&'static str, String, bool)> {
    let object = task.as_object().ok_or_else(|| {
        (
            "task_not_finalizable",
            "task entry must be an object".to_string(),
            false,
        )
    })?;
    let task_id = object
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            (
                "task_not_finalizable",
                "task_id is missing".to_string(),
                false,
            )
        })?;
    if object
        .get("meta")
        .and_then(|meta| meta.get("status"))
        .and_then(Value::as_str)
        != Some("Processed")
    {
        return Err((
            "task_not_finalizable",
            "task status is not Processed".to_string(),
            true,
        ));
    }
    let Some(payload) = decoded_payload(object.get("payload")) else {
        return Err((
            "task_not_finalizable",
            "task payload is missing or is not JSON".to_string(),
            true,
        ));
    };
    if payload.get("approval_state").and_then(Value::as_str) != Some("approved") {
        return Err((
            "approval_missing",
            "task payload approval_state is not approved".to_string(),
            true,
        ));
    }
    if payload
        .get("finalize_requested_at")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        return Err((
            "finalize_not_requested",
            "task payload has no finalize_requested_at".to_string(),
            true,
        ));
    }
    let agenda_id = payload
        .get("agenda_id")
        .or_else(|| payload.get("canopus_agenda_id"))
        .or_else(|| payload.get("run_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(task_id)
        .to_string();
    let run_id = derive_approved_run_id(&payload, &agenda_id, task_id)
        .map_err(|err| ("task_not_finalizable", err.to_string(), false))?;
    let repo_path = payload
        .get("repo_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    Ok(ApprovedProcessedTask {
        task_id: task_id.to_string(),
        agenda_id,
        run_id,
        repo_path,
    })
}

#[derive(Debug)]
struct FinalizeApprovedFailure {
    message: String,
    response: FinalizeApprovedResponse,
}

fn finalize_failure(
    task: Option<&ApprovedProcessedTask>,
    code: &'static str,
    message: impl Into<String>,
    retryable: bool,
    details: Value,
) -> FinalizeApprovedFailure {
    let message = message.into();
    FinalizeApprovedFailure {
        message: message.clone(),
        response: FinalizeApprovedResponse::failure(task, code, message, retryable, details),
    }
}

#[derive(Debug, Serialize)]
struct FinalizeApprovedResponse {
    ok: bool,
    command: &'static str,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agenda_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repo_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sidecar_path: Option<String>,
    idempotent: bool,
    retryable: bool,
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<FinalizeApprovedError>,
}

#[derive(Debug, Serialize)]
struct FinalizeApprovedError {
    code: &'static str,
    message: String,
    retryable: bool,
    details: Value,
}

impl FinalizeApprovedResponse {
    #[allow(clippy::too_many_arguments)]
    fn success(
        task: &ApprovedProcessedTask,
        status: &str,
        mode: FinalizeMode,
        repo: &Path,
        branch: &str,
        commit: Option<String>,
        sidecar_path: &Path,
        idempotent: bool,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            ok: true,
            command: "finalize-approved",
            status: status.to_string(),
            task_id: Some(task.task_id.clone()),
            agenda_id: Some(task.agenda_id.clone()),
            run_id: Some(task.run_id.clone()),
            mode: Some(mode.as_json_str().to_string()),
            repo_path: Some(repo.display().to_string()),
            branch: Some(branch.to_string()),
            commit,
            sidecar_path: Some(sidecar_path.display().to_string()),
            idempotent,
            retryable: false,
            warnings,
            error: None,
        }
    }

    fn failure(
        task: Option<&ApprovedProcessedTask>,
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        details: Value,
    ) -> Self {
        Self {
            ok: false,
            command: "finalize-approved",
            status: code.to_string(),
            task_id: task.map(|task| task.task_id.clone()),
            agenda_id: task.map(|task| task.agenda_id.clone()),
            run_id: task.map(|task| task.run_id.clone()),
            mode: None,
            repo_path: task.and_then(|task| {
                task.repo_path
                    .as_ref()
                    .map(|path| path.display().to_string())
            }),
            branch: task.map(|task| format!("canopus/{}", task.run_id)),
            commit: None,
            sidecar_path: None,
            idempotent: false,
            retryable,
            warnings: vec![],
            error: Some(FinalizeApprovedError {
                code,
                message: message.into(),
                retryable,
                details,
            }),
        }
    }
}

fn print_finalize_approved_json(response: &FinalizeApprovedResponse) -> CanopusResult<()> {
    let text = serde_json::to_string_pretty(response)
        .map_err(|err| CanopusError::Runtime(format!("serialize JSON: {err}")))?;
    println!("{text}");
    Ok(())
}

fn classify_finalize_error(err: &CanopusError) -> &'static str {
    let message = err.to_string();
    if message.contains("aborting auto-commit:") {
        "branch_preflight_failed"
    } else if message.contains("commit") {
        "git_commit_failed"
    } else {
        "internal_error"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinalizeMode {
    DryRun,
    LocalCommitOnly,
    Mutate,
}

impl FinalizeMode {
    fn as_json_str(self) -> &'static str {
        match self {
            FinalizeMode::DryRun => "dry_run",
            FinalizeMode::LocalCommitOnly => "local_commit_only",
            FinalizeMode::Mutate => "mutate",
        }
    }
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

    // LocalCommitOnly: pre-flight + commit on the existing submit-created branch,
    // then return without push/PR.
    // V1.5 limitations (documented in plan): user_request, reviewer summary, and commit_type
    // inference are not yet wired through to this call site, so we use minimum viable values.
    if matches!(mode, FinalizeMode::LocalCommitOnly) {
        // Pre-flight 1: non-detached HEAD (post-executor invariant).
        let head = tools.run_check(repo, &["git", "symbolic-ref", "-q", "HEAD"])?;
        if head.status != 0 {
            return Err(CanopusError::Runtime(
                "aborting auto-commit: detached HEAD".to_string(),
            ));
        }
        let current_branch = head
            .stdout
            .trim()
            .strip_prefix("refs/heads/")
            .unwrap_or_else(|| head.stdout.trim())
            .to_string();
        if matches!(current_branch.as_str(), "main" | "master" | "develop") {
            return Err(CanopusError::Runtime(format!(
                "aborting auto-commit: protected branch {current_branch}"
            )));
        }
        if current_branch != branch {
            return Err(CanopusError::Runtime(format!(
                "aborting auto-commit: current branch {current_branch} does not match expected {branch}"
            )));
        }
        // Pre-flight 2: clean index (post-executor invariant; distinct from submit-time
        // ensure_clean_worktree which validates pre-executor state).
        let cached = tools.run_check(repo, &["git", "diff", "--cached", "--quiet"])?;
        if cached.status != 0 {
            return Err(CanopusError::Runtime(
                "aborting auto-commit: index is dirty".to_string(),
            ));
        }
        // Pre-flight 3: .canopus/ must be gitignored (PM-a; sidesteps changed_files
        // rename-arrow bug while keeping `git add -A` safe).
        let ignored = tools.run_check(repo, &["git", "check-ignore", "-q", ".canopus/"])?;
        if ignored.status != 0 {
            return Err(CanopusError::Runtime(
                "aborting auto-commit: .canopus/ must be in .gitignore".to_string(),
            ));
        }

        // Capture porcelain before commit so no-op finalization is idempotent.
        let porcelain = tools.changed_files(repo)?;
        let changed_paths: Vec<PathBuf> = porcelain
            .stdout
            .lines()
            .filter_map(|line| {
                // Porcelain format: "XY path" — first 3 chars are status, rest is path.
                line.get(3..).map(|p| PathBuf::from(p.trim()))
            })
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        if changed_paths.is_empty() {
            plan.push("local-commit-only: no changes to commit; idempotent skip".to_string());
            log::info!(
                "[finalize] local-commit-only: no changes for run {}; skipping",
                agenda_id
            );
            return Ok(plan.join("\n"));
        }

        let user_request = "";
        // Build commit message via the pure formatter.
        // V1.5 minimum viable inputs: empty reviewer_summary/body, hardcoded "feat" type,
        // legacy summary pattern, empty user_request. Type inference + reviewer artifact
        // wiring tracked as follow-up.
        let modules = derive_modules(&changed_paths);
        let summary = format!("complete agenda {}", agenda_id);
        let runtime = std::env::var("CANOPUS_AGENT_RUNTIME").unwrap_or_else(|_| "mock".into());
        let model = match runtime.as_str() {
            "codex" => std::env::var("CANOPUS_CODEX_MODEL").unwrap_or_else(|_| "default".into()),
            "claude" => std::env::var("CANOPUS_CLAUDE_MODEL").unwrap_or_else(|_| "default".into()),
            _ => "n/a".into(),
        };
        let msg = format_commit_message(
            "",
            &modules,
            "feat",
            &summary,
            "",
            user_request,
            &runtime,
            &model,
        );

        tools.run_check(repo, &["git", "add", "-A"])?;
        let commit = tools.run_check(repo, &["git", "commit", "-m", &msg])?;
        if commit.status != 0 {
            let no_changes = commit.stdout.contains("nothing to commit")
                || commit.stderr.contains("nothing to commit")
                || commit.stdout.contains("no changes added")
                || commit.stderr.contains("no changes added");
            if no_changes {
                plan.push(
                    "local-commit-only: no changes after add; continuing idempotently".to_string(),
                );
            } else {
                return Err(CanopusError::Tool(commit.stderr));
            }
        }
        log::info!(
            "[finalize] local-commit-only: branch={} run_id={}",
            current_branch,
            agenda_id
        );
        if commit.status == 0 {
            let sha = latest_commit(repo)?;
            plan.push(format!("commit: {sha}"));
        }
        plan.push(format!(
            "local-commit-only: committed on branch {} (no push, no PR)",
            current_branch
        ));
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

fn persist_finalize_record_for_mode(
    state: &Path,
    run_id: &str,
    output: &str,
    mode: FinalizeMode,
) -> CanopusResult<()> {
    let path = finalize_record_path(state, run_id);
    if should_skip_existing_finalize_record(&path, mode) {
        log::info!(
            "[watch] terminal finalize record exists after {:?}; preserving {}",
            mode,
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

fn should_skip_existing_finalize_record(path: &Path, mode: FinalizeMode) -> bool {
    if !path.exists() {
        return false;
    }
    if matches!(mode, FinalizeMode::DryRun) {
        return true;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    text.contains("finalize mode: LocalCommitOnly")
        && (text.contains("commit: ") || text.contains("no changes"))
}

fn should_skip_existing_finalize_record_for_branch(
    path: &Path,
    mode: FinalizeMode,
    branch: &str,
) -> bool {
    if !should_skip_existing_finalize_record(path, mode) {
        return false;
    }
    if matches!(mode, FinalizeMode::DryRun) {
        return true;
    }
    fs::read_to_string(path)
        .map(|text| text.contains(&format!("branch: {branch}")))
        .unwrap_or(false)
}

fn read_commit_from_record(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("commit: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn current_commit(tools: &LocalToolGateway, repo: &Path) -> Option<String> {
    let output = tools.run_check(repo, &["git", "rev-parse", "HEAD"]).ok()?;
    (output.status == 0).then(|| output.stdout.trim().to_string())
}

fn latest_commit(repo: &Path) -> CanopusResult<String> {
    let tools = LocalToolGateway;
    let output = tools.run_check(repo, &["git", "rev-parse", "HEAD"])?;
    if output.status != 0 {
        return Err(CanopusError::Tool(output.stderr));
    }
    Ok(output.stdout.trim().to_string())
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

fn notify_discord_token_usage(state: &std::path::Path, run_id: &str) {
    let path = state
        .join("runs")
        .join(format!("{run_id}-token-usage.json"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let input = v["input_tokens"].as_u64().unwrap_or(0);
    let output = v["output_tokens"].as_u64().unwrap_or(0);
    let total = v["total_tokens"].as_u64().unwrap_or(input + output);
    if total == 0 {
        return;
    }
    let msg = format!(
        "🪙 **{total}** tokens (input: {:.1}k / output: {:.1}k)",
        input as f64 / 1000.0,
        output as f64 / 1000.0,
    );
    notify_discord(&msg);
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
                    agenda_id: "agenda-discord-string".to_string(),
                    run_id: "agenda-discord-string-discord-string".to_string(),
                    repo_path: None
                },
                ApprovedProcessedTask {
                    task_id: "discord-object".to_string(),
                    agenda_id: "agenda-discord-object".to_string(),
                    run_id: "agenda-discord-object-discord-object".to_string(),
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
                agenda_id: "agenda-discord-object".to_string(),
                run_id: "agenda-discord-object-discord-object".to_string(),
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
        let record = fs::read_to_string(finalize_record_path(
            &payload_state,
            "agenda-payload-discord-payload",
        ))
        .unwrap();
        assert!(record.contains("payload.txt"));
        assert!(!record.contains("fallback.txt"));
        assert!(
            !finalize_record_path(&state, "agenda-payload-discord-payload").exists(),
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
