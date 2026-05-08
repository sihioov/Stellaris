use dysonsphere::{
    error::{Result, StellarisError},
    message::{TaskMessage, TaskType},
};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanopusRoleMode {
    Agent,
    Planner,
    Reviewer,
}

impl CanopusRoleMode {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "canopus.agent" => Some(Self::Agent),
            "canopus.planner" => Some(Self::Planner),
            "canopus.reviewer" => Some(Self::Reviewer),
            _ => None,
        }
    }

    fn as_arg(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Planner => "planner",
            Self::Reviewer => "reviewer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CanopusMetadata {
    request: String,
    repo_path: Option<String>,
    agenda_id: Option<String>,
    github_issue_url: Option<String>,
    github_issue_number: Option<String>,
    github_project_id: Option<String>,
    github_project_url: Option<String>,
    github_project_item_id: Option<String>,
    github_project_status: Option<String>,
    github_project_owner_kind: Option<String>,
    github_project_owner: Option<String>,
    github_project_number: Option<String>,
    github_project_status_field_id: Option<String>,
    github_project_status_field_name: Option<String>,
    github_project_status_option_id: Option<String>,
    github_project_status_option_name: Option<String>,
    github_project_mode: Option<String>,
}

pub async fn handle(task: &TaskMessage, label: &str) -> Result<()> {
    let Some(role_mode) = CanopusRoleMode::from_label(label) else {
        return Err(StellarisError::IoError(format!(
            "unsupported custom task label `{label}` for task {}",
            task.task_id
        )));
    };

    run_canopus(task, role_mode).await
}

/// Public entry point used by integration tests in sibling crates (e.g.
/// `apps/canopus/tests/multi_project_state_routing.rs` per plan §6.2 / PR-A
/// A2). Hides the internal [`CanopusRoleMode`] enum behind a string label so
/// the public surface stays minimal.
pub fn canopus_submit_args_for_label(
    task: &TaskMessage,
    repo: &str,
    state: &str,
    label: &str,
) -> Result<Vec<String>> {
    let role_mode = CanopusRoleMode::from_label(label).ok_or_else(|| {
        StellarisError::IoError(format!(
            "unsupported custom task label `{label}` for task {}",
            task.task_id
        ))
    })?;
    Ok(canopus_submit_args(task, repo, state, role_mode))
}

fn canopus_submit_args(
    task: &TaskMessage,
    repo: &str,
    state: &str,
    role_mode: CanopusRoleMode,
) -> Vec<String> {
    let metadata = parse_canopus_metadata(&task.payload);
    let agenda_id = metadata
        .agenda_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&task.task_id);
    let request = if metadata.request.trim().is_empty() {
        task.payload.as_str()
    } else {
        metadata.request.as_str()
    };
    let payload_repo: Option<&str> = metadata
        .repo_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let submit_repo = payload_repo.unwrap_or(repo);
    let (submit_state, state_source) = derive_submit_state(payload_repo, state);
    log::info!(
        "[submit] state_root resolved: {} (source={})",
        submit_state,
        state_source
    );

    let mut args = vec![
        "submit".to_string(),
        "--repo".to_string(),
        submit_repo.to_string(),
        "--state".to_string(),
        submit_state,
        "--agenda-id".to_string(),
        agenda_id.to_string(),
        "--task-id".to_string(),
        task.task_id.clone(),
        "--task-type".to_string(),
        task_type_arg(&task.task_type),
        "--role-mode".to_string(),
        role_mode.as_arg().to_string(),
        "--task-status".to_string(),
        format!("{:?}", task.meta.status),
        "--task-created-at".to_string(),
        task.meta.created_at.to_rfc3339(),
        "--task-updated-at".to_string(),
        task.meta.updated_at.to_rfc3339(),
    ];

    if let Some(url) = metadata
        .github_issue_url
        .filter(|value| !value.trim().is_empty())
    {
        args.extend(["--github-issue-url".to_string(), url]);
    }
    if let Some(number) = metadata
        .github_issue_number
        .filter(|value| !value.trim().is_empty())
    {
        args.extend(["--github-issue-number".to_string(), number]);
    }
    push_optional_arg(&mut args, "--github-project-id", metadata.github_project_id);
    push_optional_arg(
        &mut args,
        "--github-project-url",
        metadata.github_project_url,
    );
    push_optional_arg(
        &mut args,
        "--github-project-item-id",
        metadata.github_project_item_id,
    );
    push_optional_arg(
        &mut args,
        "--github-project-status",
        metadata.github_project_status,
    );
    push_optional_arg(
        &mut args,
        "--github-project-owner-kind",
        metadata.github_project_owner_kind,
    );
    push_optional_arg(
        &mut args,
        "--github-project-owner",
        metadata.github_project_owner,
    );
    push_optional_arg(
        &mut args,
        "--github-project-number",
        metadata.github_project_number,
    );
    push_optional_arg(
        &mut args,
        "--github-project-status-field-id",
        metadata.github_project_status_field_id,
    );
    push_optional_arg(
        &mut args,
        "--github-project-status-field-name",
        metadata.github_project_status_field_name,
    );
    push_optional_arg(
        &mut args,
        "--github-project-status-option-id",
        metadata.github_project_status_option_id,
    );
    push_optional_arg(
        &mut args,
        "--github-project-status-option-name",
        metadata.github_project_status_option_name,
    );
    push_optional_arg(
        &mut args,
        "--github-project-mode",
        metadata.github_project_mode,
    );

    args.push(request.to_string());
    args
}

fn push_optional_arg(args: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        args.extend([flag.to_string(), value]);
    }
}

/// Derives the `--state` argument for `canopus submit` from the task
/// payload's `repo_path` (plan §1.2 / §7 PR-A). When payload provides a repo
/// and the candidate `<repo>/.canopus` is creatable + writable, returns it
/// with `source=payload_repo`; otherwise returns `env_state` and emits a
/// WARN line so operators can grep the cause (plan §5.5).
///
/// Mirrors `canopus::cli::derive_state_with_source` but operates on `&str`
/// because laniakea builds CLI args. Cross-crate dependency is intentionally
/// avoided to keep the dispatcher independent (plan principle #4 minimum
/// diff; canopus already has `laniakea` as a dev-dep, so reversing the
/// direction would create a cycle).
fn derive_submit_state(payload_repo: Option<&str>, env_state: &str) -> (String, &'static str) {
    if let Some(repo) = payload_repo {
        let candidate = Path::new(repo).join(".canopus");
        match probe_state_writable(&candidate) {
            Ok(()) => {
                let resolved = candidate
                    .into_os_string()
                    .into_string()
                    .unwrap_or_else(|_| format!("{}/.canopus", repo.trim_end_matches('/')));
                return (resolved, "payload_repo");
            }
            Err(err) => {
                log::warn!(
                    "[submit] state_root probe failed at {}/.canopus: {}; falling back to env_state",
                    repo,
                    err
                );
            }
        }
    }
    (env_state.to_string(), "env_fallback")
}

fn probe_state_writable(candidate: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(candidate)?;
    let probe = candidate.join(".write-probe");
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

fn parse_canopus_metadata(payload: &str) -> CanopusMetadata {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
        if let Some(object) = value.as_object() {
            let get = |key: &str| {
                object.get(key).and_then(|value| match value {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
            };

            return CanopusMetadata {
                request: get("request")
                    .or_else(|| get("prompt"))
                    .or_else(|| get("title"))
                    .unwrap_or_else(|| payload.to_string()),
                repo_path: get("repo_path"),
                agenda_id: get("agenda_id"),
                github_issue_url: get("github_issue_url"),
                github_issue_number: get("github_issue_number")
                    .or_else(|| get("github_issue"))
                    .or_else(|| get("issue_number")),
                github_project_id: get("github_project_id"),
                github_project_url: get("github_project_url"),
                github_project_item_id: get("github_project_item_id"),
                github_project_status: get("github_project_status"),
                github_project_owner_kind: get("github_project_owner_kind"),
                github_project_owner: get("github_project_owner"),
                github_project_number: get("github_project_number"),
                github_project_status_field_id: get("github_project_status_field_id"),
                github_project_status_field_name: get("github_project_status_field_name"),
                github_project_status_option_id: get("github_project_status_option_id"),
                github_project_status_option_name: get("github_project_status_option_name"),
                github_project_mode: get("github_project_mode"),
            };
        }
    }

    let mut metadata = CanopusMetadata {
        request: payload.to_string(),
        ..CanopusMetadata::default()
    };

    for line in payload.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "prompt" | "request" => metadata.request = value,
            "repo_path" => metadata.repo_path = Some(value),
            "agenda_id" => metadata.agenda_id = Some(value),
            "github_issue_url" => metadata.github_issue_url = Some(value),
            "github_issue_number" | "github_issue" | "issue_number" => {
                metadata.github_issue_number = Some(value)
            }
            "github_project_id" => metadata.github_project_id = Some(value),
            "github_project_url" => metadata.github_project_url = Some(value),
            "github_project_item_id" => metadata.github_project_item_id = Some(value),
            "github_project_status" => metadata.github_project_status = Some(value),
            "github_project_owner_kind" => metadata.github_project_owner_kind = Some(value),
            "github_project_owner" => metadata.github_project_owner = Some(value),
            "github_project_number" => metadata.github_project_number = Some(value),
            "github_project_status_field_id" => {
                metadata.github_project_status_field_id = Some(value)
            }
            "github_project_status_field_name" => {
                metadata.github_project_status_field_name = Some(value)
            }
            "github_project_status_option_id" => {
                metadata.github_project_status_option_id = Some(value)
            }
            "github_project_status_option_name" => {
                metadata.github_project_status_option_name = Some(value)
            }
            "github_project_mode" => metadata.github_project_mode = Some(value),
            _ => {}
        }
    }

    metadata
}

fn task_type_arg(task_type: &TaskType) -> String {
    match task_type {
        TaskType::NewsA => "news-a".to_string(),
        TaskType::Custom(label) => format!("custom:{label}"),
        TaskType::Bug => "bug".to_string(),
        TaskType::Security => "security".to_string(),
        TaskType::TestCoverage => "test-coverage".to_string(),
        TaskType::UXImprovement => "ux-improvement".to_string(),
    }
}

async fn run_canopus(task: &TaskMessage, role_mode: CanopusRoleMode) -> Result<()> {
    let repo = std::env::var("CANOPUS_REPO_PATH").unwrap_or_else(|_| ".".into());
    let state = std::env::var("CANOPUS_STATE_PATH").unwrap_or_else(|_| ".canopus".into());
    let timeout_secs = std::env::var("CANOPUS_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3600);

    notify_discord(&format!("🚀 **작업 시작**: {}", task.payload));
    log::info!(
        "[Canopus] Starting for task {} ({:?}): {}",
        task.task_id,
        role_mode,
        task.payload
    );

    let args = canopus_submit_args(task, &repo, &state, role_mode);
    let cmd_future = Command::new("canopus").args(args).output();

    let output = match tokio::time::timeout(Duration::from_secs(timeout_secs), cmd_future).await {
        Err(_) => {
            log::error!(
                "[Canopus] Timed out after {}s for task {}",
                timeout_secs,
                task.task_id
            );
            notify_discord(&format!(
                "⏰ **타임아웃** task_id={} ({}s)",
                task.task_id, timeout_secs
            ));
            return Err(StellarisError::IoError(format!(
                "canopus timed out after {timeout_secs}s"
            )));
        }
        Ok(result) => result,
    };

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            log::info!("[Canopus] Output: {}", stdout);
            notify_discord(&format!(
                "✅ **작업 완료 — 검토 대기 중** `task_id={}`\n`!approve {}` 또는 `!reject {}`로 응답해주세요.",
                task.task_id, task.task_id, task.task_id
            ));
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            log::error!("[Canopus] Exited with {}: {}", out.status, stderr);
            notify_discord(&format!(
                "❌ **작업 실패** (exit {}): {}",
                out.status,
                stderr.chars().take(200).collect::<String>()
            ));
            Err(StellarisError::IoError(format!(
                "canopus exited with {}",
                out.status
            )))
        }
        Err(e) => {
            log::error!("[Canopus] Failed to launch: {e}");
            notify_discord(&format!("❌ **canopus 실행 실패**: {}", e));
            Err(StellarisError::IoError(e.to_string()))
        }
    }
}

fn notify_discord(message: &str) {
    if let Ok(url) = std::env::var("DISCORD_WEBHOOK_URL") {
        let body = serde_json::json!({"content": message});
        // fire-and-forget: ureq is sync, spawn_blocking avoids blocking the async executor
        tokio::task::spawn_blocking(move || {
            if let Err(e) = ureq::post(&url).send_json(body) {
                log::warn!("[Canopus] Discord notify failed: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dysonsphere::message::{TaskMessage, TaskMeta, TaskType};

    fn task(task_type: TaskType, payload: &str) -> TaskMessage {
        TaskMessage {
            task_id: "UPSTREAM-42".to_string(),
            task_type,
            payload: payload.to_string(),
            meta: TaskMeta::default(),
        }
    }

    fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].as_str())
    }

    #[tokio::test]
    async fn unsupported_custom_label_fails_loudly() {
        let err = handle(
            &task(TaskType::Custom("foo".to_string()), "fix routing"),
            "foo",
        )
        .await
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("unsupported custom task label `foo`"));
    }

    #[test]
    fn canopus_submit_args_include_upstream_metadata_and_role_mode() {
        let payload = serde_json::json!({
            "request": "fix routed bug",
            "agenda_id": "AGENDA-7",
            "github_issue_url": "https://github.example/owner/repo/issues/7",
            "github_issue_number": 7,
            "github_project_id": "PVT_project",
            "github_project_url": "https://github.com/orgs/acme/projects/1",
            "github_project_status": "Ready",
            "github_project_owner_kind": "org",
            "github_project_owner": "acme",
            "github_project_number": 1,
            "github_project_status_field_name": "Status",
            "github_project_status_option_name": "Ready",
            "github_project_mode": "dry-run-offline"
        })
        .to_string();
        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(args[0], "submit");
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--agenda-id", "AGENDA-7"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--task-id", "UPSTREAM-42"]));
        assert!(args.windows(2).any(|pair| pair == ["--task-type", "bug"]));
        assert!(args.windows(2).any(|pair| pair == ["--role-mode", "agent"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--task-status", "Pending"]));
        assert!(args.windows(2).any(|pair| pair[0] == "--task-created-at"));
        assert!(args.windows(2).any(|pair| pair[0] == "--task-updated-at"));
        assert!(args.windows(2).any(|pair| pair
            == [
                "--github-issue-url",
                "https://github.example/owner/repo/issues/7"
            ]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-issue-number", "7"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-id", "PVT_project"]));
        assert!(args.windows(2).any(|pair| pair
            == [
                "--github-project-url",
                "https://github.com/orgs/acme/projects/1"
            ]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-status", "Ready"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-owner-kind", "org"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-number", "1"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-status-field-name", "Status"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-status-option-name", "Ready"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-mode", "dry-run-offline"]));
        assert!(!args.iter().any(|arg| arg == "--allow-github-mutation"));
        assert!(!args.iter().any(|arg| arg == "--allow-mutation"));
        assert_eq!(args.last().unwrap(), "fix routed bug");
    }

    #[test]
    fn canopus_handoff_does_not_invent_live_mutation_authority() {
        let payload = serde_json::json!({
            "request": "keep policy in canopus",
            "agenda_id": "AGENDA-8",
            "github_project_id": "PVT_project",
            "github_project_item_id": "PVTI_existing",
            "github_project_mode": "validate-read-only"
        })
        .to_string();
        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--github-project-mode", "validate-read-only"]));
        assert!(!args.iter().any(|arg| arg == "--allow-github-mutation"));
        assert!(!args.iter().any(|arg| arg == "--allow-mutation"));
    }

    #[test]
    fn json_payload_repo_path_overrides_submit_repo_fallback() {
        let payload = serde_json::json!({
            "request": "run in selected worktree",
            "repo_path": "/worktrees/smoke"
        })
        .to_string();
        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some("/worktrees/smoke"));
        assert_eq!(args.last().unwrap(), "run in selected worktree");
    }

    #[test]
    fn key_value_payload_repo_path_overrides_submit_repo_fallback() {
        let args = canopus_submit_args(
            &task(
                TaskType::Bug,
                "request=run legacy payload\nrepo_path=/worktrees/legacy",
            ),
            "/repo-from-env",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some("/worktrees/legacy"));
        assert_eq!(args.last().unwrap(), "run legacy payload");
    }

    #[test]
    fn missing_payload_repo_path_uses_submit_repo_fallback() {
        let payload = serde_json::json!({
            "request": "run in default repo"
        })
        .to_string();
        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some("/repo-from-env"));
        assert_eq!(args.last().unwrap(), "run in default repo");
    }

    #[test]
    fn blank_payload_repo_path_uses_submit_repo_fallback() {
        let payload = serde_json::json!({
            "request": "run with blank repo metadata",
            "repo_path": "   "
        })
        .to_string();
        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some("/repo-from-env"));
        assert_eq!(args.last().unwrap(), "run with blank repo metadata");
    }

    #[test]
    fn planner_and_reviewer_labels_select_intentional_role_modes() {
        let planner = canopus_submit_args(
            &task(
                TaskType::Custom("canopus.planner".to_string()),
                "prompt=make a plan",
            ),
            "/repo",
            "/state",
            CanopusRoleMode::Planner,
        );
        let reviewer = canopus_submit_args(
            &task(
                TaskType::Custom("canopus.reviewer".to_string()),
                "prompt=review this",
            ),
            "/repo",
            "/state",
            CanopusRoleMode::Reviewer,
        );

        assert!(planner
            .windows(2)
            .any(|pair| pair == ["--task-type", "custom:canopus.planner"]));
        assert!(planner
            .windows(2)
            .any(|pair| pair == ["--role-mode", "planner"]));
        assert_eq!(planner.last().unwrap(), "make a plan");
        assert!(reviewer
            .windows(2)
            .any(|pair| pair == ["--task-type", "custom:canopus.reviewer"]));
        assert!(reviewer
            .windows(2)
            .any(|pair| pair == ["--role-mode", "reviewer"]));
        assert_eq!(reviewer.last().unwrap(), "review this");
    }

    #[test]
    fn plain_payload_uses_task_id_as_agenda_fallback() {
        let args = canopus_submit_args(
            &task(TaskType::Security, "audit auth"),
            "/repo",
            "/state",
            CanopusRoleMode::Agent,
        );

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--agenda-id", "UPSTREAM-42"]));
        assert_eq!(args.last().unwrap(), "audit auth");
    }

    fn unique_tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("laniakea-{name}-{}", std::process::id()))
    }

    #[test]
    fn derive_submit_state_returns_payload_canopus_when_writable() {
        // PR-A §5.1/§7: payload-driven state derivation, principle #1 SSOT.
        let repo = unique_tmp("derive-submit-state-payload");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();

        let (state, source) = derive_submit_state(Some(repo.to_str().unwrap()), "/env-state");

        assert_eq!(state, repo.join(".canopus").to_str().unwrap());
        assert_eq!(source, "payload_repo");
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn derive_submit_state_falls_back_to_env_when_payload_absent() {
        // principle #3 backwards compat: no payload repo → env_state preserved.
        let (state, source) = derive_submit_state(None, "/env-state");

        assert_eq!(state, "/env-state");
        assert_eq!(source, "env_fallback");
    }

    #[test]
    fn derive_submit_state_falls_back_when_probe_fails() {
        // §5.5 scenario E: probe failure must surface env_fallback + WARN.
        // /proc is a virtual fs that rejects creation under it for non-root.
        let (state, source) =
            derive_submit_state(Some("/proc/1/canopus-write-probe"), "/env-state");

        assert_eq!(state, "/env-state");
        assert_eq!(source, "env_fallback");
    }

    #[test]
    fn canopus_submit_args_uses_payload_repo_for_state_when_writable() {
        let repo = unique_tmp("submit-args-payload-state");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let payload = serde_json::json!({
            "request": "run in selected worktree",
            "repo_path": repo.to_str().unwrap()
        })
        .to_string();

        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/env-state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some(repo.to_str().unwrap()));
        assert_eq!(
            arg_value(&args, "--state"),
            Some(repo.join(".canopus").to_str().unwrap())
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn canopus_submit_args_state_falls_back_to_env_when_payload_repo_absent() {
        let payload = serde_json::json!({"request": "use env state"}).to_string();

        let args = canopus_submit_args(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/env-state",
            CanopusRoleMode::Agent,
        );

        assert_eq!(arg_value(&args, "--repo"), Some("/repo-from-env"));
        assert_eq!(arg_value(&args, "--state"), Some("/env-state"));
    }

    #[test]
    fn canopus_submit_args_for_label_exposes_args_to_external_callers() {
        // PR-A A2: integration tests in apps/canopus/tests/multi_project_state_routing.rs
        // need to call this without seeing CanopusRoleMode.
        let repo = unique_tmp("submit-args-public-label");
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        let payload = serde_json::json!({
            "request": "labeled call",
            "repo_path": repo.to_str().unwrap()
        })
        .to_string();

        let args = canopus_submit_args_for_label(
            &task(TaskType::Bug, &payload),
            "/repo-from-env",
            "/env-state",
            "canopus.agent",
        )
        .unwrap();

        assert_eq!(arg_value(&args, "--repo"), Some(repo.to_str().unwrap()));
        assert_eq!(
            arg_value(&args, "--state"),
            Some(repo.join(".canopus").to_str().unwrap())
        );

        let err = canopus_submit_args_for_label(
            &task(TaskType::Bug, "{}"),
            "/repo",
            "/state",
            "not-a-real-label",
        )
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("unsupported custom task label `not-a-real-label`"));
        let _ = std::fs::remove_dir_all(&repo);
    }
}
