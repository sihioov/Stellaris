use crate::adapters::github::ProjectOwnerKind;
use crate::core::{CanopusError, CanopusResult, GitHubProjectMode};
use dysonsphere::message::TaskType;
use std::fs;
use std::path::{Path, PathBuf};

/// Origin of the resolved state directory used by [`derive_state_for_run`].
///
/// Surfaced so callers can attach `source=...` to their observability log lines
/// (plan §6.5) without re-deriving the source label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSource {
    /// Returned when the payload `repo_path` directory exists, is writable, and
    /// resolved to `<payload_repo>/.canopus`.
    PayloadRepo,
    /// Returned when payload `repo_path` is absent, missing, or fails the
    /// write probe (plan §5.5). Caller falls back to the env/CLI-provided
    /// state path.
    EnvFallback,
}

impl StateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PayloadRepo => "payload_repo",
            Self::EnvFallback => "env_fallback",
        }
    }
}

/// Derives the state root for a finalize/submit run from the task payload's
/// `repo_path`, falling back to `parsed_state` when payload is absent or the
/// candidate directory is not writable.
///
/// When `payload_repo` is provided and `<payload_repo>/.canopus` is creatable
/// and writable (verified via a `.write-probe` round trip per plan §5.5),
/// this returns that path. Otherwise it returns `parsed_state` and emits a
/// WARN log so operators can grep for the cause.
pub fn derive_state_for_run(payload_repo: Option<&Path>, parsed_state: &Path) -> PathBuf {
    derive_state_with_source(payload_repo, parsed_state).0
}

/// Variant of [`derive_state_for_run`] that also returns a [`StateSource`]
/// label for observability (plan §6.5 / PR-A A7).
pub fn derive_state_with_source(
    payload_repo: Option<&Path>,
    parsed_state: &Path,
) -> (PathBuf, StateSource) {
    if let Some(repo) = payload_repo {
        let candidate = repo.join(".canopus");
        match probe_state_writable(&candidate) {
            Ok(()) => return (candidate, StateSource::PayloadRepo),
            Err(err) => {
                log::warn!(
                    "[submit] state_root probe failed at {}: {}; falling back to env_state",
                    candidate.display(),
                    err
                );
            }
        }
    }
    (parsed_state.to_path_buf(), StateSource::EnvFallback)
}

fn probe_state_writable(candidate: &Path) -> std::io::Result<()> {
    fs::create_dir_all(candidate)?;
    let probe = candidate.join(".write-probe");
    fs::write(&probe, b"")?;
    fs::remove_file(&probe)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct WorktreeCreateArgs {
    pub(crate) repo: PathBuf,
    pub(crate) name: String,
    pub(crate) path: Option<PathBuf>,
}

impl WorktreeCreateArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = None;
        let mut name = None;
        let mut path = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = Some(PathBuf::from(required_value(args, index, "--repo")?));
                }
                "--name" => {
                    index += 1;
                    name = Some(required_value(args, index, "--name")?.to_string());
                }
                "--path" => {
                    index += 1;
                    path = Some(PathBuf::from(required_value(args, index, "--path")?));
                }
                "--json" => {}
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown worktree create argument: {value}"
                    )))
                }
            }
            index += 1;
        }
        Ok(Self {
            repo: repo.ok_or_else(|| {
                CanopusError::InvalidInput("worktree create requires --repo".to_string())
            })?,
            name: name.ok_or_else(|| {
                CanopusError::InvalidInput("worktree create requires --name".to_string())
            })?,
            path,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectRegisterArgs {
    pub(crate) repo: PathBuf,
    pub(crate) github_owner: String,
    pub(crate) github_repo: String,
    pub(crate) project_owner_kind: ProjectOwnerKind,
    pub(crate) project_owner: String,
    pub(crate) create_github_repo: bool,
}

impl ProjectRegisterArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = None;
        let mut github_owner = None;
        let mut github_repo = None;
        let mut project_owner_kind = None;
        let mut project_owner = None;
        let mut create_github_repo = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = Some(PathBuf::from(required_value(args, index, "--repo")?));
                }
                "--github-owner" => {
                    index += 1;
                    github_owner = Some(required_value(args, index, "--github-owner")?.to_string());
                }
                "--github-repo" => {
                    index += 1;
                    github_repo = Some(required_value(args, index, "--github-repo")?.to_string());
                }
                "--project-owner-kind" => {
                    index += 1;
                    project_owner_kind = Some(ProjectOwnerKind::parse(required_value(
                        args,
                        index,
                        "--project-owner-kind",
                    )?)?);
                }
                "--project-owner" => {
                    index += 1;
                    project_owner =
                        Some(required_value(args, index, "--project-owner")?.to_string());
                }
                "--create-github-repo" => create_github_repo = true,
                "--json" => {}
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown project-register argument: {value}"
                    )))
                }
            }
            index += 1;
        }
        Ok(Self {
            repo: repo.ok_or_else(|| {
                CanopusError::InvalidInput("project-register requires --repo".to_string())
            })?,
            github_owner: github_owner.ok_or_else(|| {
                CanopusError::InvalidInput("project-register requires --github-owner".to_string())
            })?,
            github_repo: github_repo.ok_or_else(|| {
                CanopusError::InvalidInput("project-register requires --github-repo".to_string())
            })?,
            project_owner_kind: project_owner_kind.ok_or_else(|| {
                CanopusError::InvalidInput(
                    "project-register requires --project-owner-kind".to_string(),
                )
            })?,
            project_owner: project_owner.ok_or_else(|| {
                CanopusError::InvalidInput("project-register requires --project-owner".to_string())
            })?,
            create_github_repo,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkIntakeArgs {
    pub(crate) repo: PathBuf,
    pub(crate) registration: String,
    pub(crate) task_id: String,
    pub(crate) agenda_id: String,
    pub(crate) request: String,
    pub(crate) project_sync: ProjectSyncPolicy,
    pub(crate) discord_message_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectSyncPolicy {
    Off,
    BestEffort,
    Required,
}

impl ProjectSyncPolicy {
    pub(crate) fn parse(value: &str) -> CanopusResult<Self> {
        match value {
            "off" => Ok(Self::Off),
            "best-effort" => Ok(Self::BestEffort),
            "required" => Ok(Self::Required),
            _ => Err(CanopusError::InvalidInput(format!(
                "unsupported work-intake --project-sync `{value}` (expected off, best-effort, or required)"
            ))),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::BestEffort => "best-effort",
            Self::Required => "required",
        }
    }
}

impl WorkIntakeArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = None;
        let mut registration = None;
        let mut task_id = None;
        let mut agenda_id = None;
        let mut request = None;
        let mut project_sync = ProjectSyncPolicy::BestEffort;
        let mut discord_message_url = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = Some(PathBuf::from(required_value(args, index, "--repo")?));
                }
                "--registration" => {
                    index += 1;
                    registration = Some(required_value(args, index, "--registration")?.to_string());
                }
                "--task-id" => {
                    index += 1;
                    task_id = Some(required_value(args, index, "--task-id")?.to_string());
                }
                "--agenda-id" => {
                    index += 1;
                    agenda_id = Some(required_value(args, index, "--agenda-id")?.to_string());
                }
                "--request" => {
                    index += 1;
                    request = Some(required_value(args, index, "--request")?.to_string());
                }
                "--project-sync" => {
                    index += 1;
                    project_sync =
                        ProjectSyncPolicy::parse(required_value(args, index, "--project-sync")?)?;
                }
                "--discord-message-url" => {
                    index += 1;
                    discord_message_url =
                        Some(required_value(args, index, "--discord-message-url")?.to_string());
                }
                "--json" => {}
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown work-intake argument: {value}"
                    )))
                }
            }
            index += 1;
        }
        Ok(Self {
            repo: repo.ok_or_else(|| {
                CanopusError::InvalidInput("work-intake requires --repo".to_string())
            })?,
            registration: registration.ok_or_else(|| {
                CanopusError::InvalidInput("work-intake requires --registration".to_string())
            })?,
            task_id: task_id.ok_or_else(|| {
                CanopusError::InvalidInput("work-intake requires --task-id".to_string())
            })?,
            agenda_id: agenda_id.ok_or_else(|| {
                CanopusError::InvalidInput("work-intake requires --agenda-id".to_string())
            })?,
            request: request.ok_or_else(|| {
                CanopusError::InvalidInput("work-intake requires --request".to_string())
            })?,
            project_sync,
            discord_message_url,
        })
    }

    pub(crate) fn registration_json(
        &self,
    ) -> CanopusResult<serde_json::Map<String, serde_json::Value>> {
        let value = if self.registration.trim_start().starts_with('{') {
            serde_json::from_str::<serde_json::Value>(&self.registration)
        } else {
            let text = fs::read_to_string(&self.registration)?;
            serde_json::from_str::<serde_json::Value>(&text)
        }
        .map_err(|e| CanopusError::InvalidInput(format!("invalid registration JSON: {e}")))?;
        value.as_object().cloned().ok_or_else(|| {
            CanopusError::InvalidInput("registration JSON must be an object".to_string())
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeliveryFinalizeArgs {
    pub(crate) repo: PathBuf,
    pub(crate) discord_approved: bool,
    pub(crate) github_ready: bool,
    pub(crate) merge_requested: bool,
    pub(crate) merge_succeeded: bool,
    pub(crate) deploy_required: bool,
}

impl DeliveryFinalizeArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = PathBuf::from(".");
        let mut discord_approved = false;
        let mut github_ready = false;
        let mut merge_requested = false;
        let mut merge_succeeded = false;
        let mut deploy_required = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = PathBuf::from(required_value(args, index, "--repo")?);
                }
                "--discord-approved" => discord_approved = true,
                "--github-ready" => github_ready = true,
                "--merge" => merge_requested = true,
                "--merge-succeeded" => merge_succeeded = true,
                "--deploy-required" => deploy_required = true,
                "--json" => {}
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown delivery-finalize argument: {value}"
                    )))
                }
            }
            index += 1;
        }
        Ok(Self {
            repo,
            discord_approved,
            github_ready,
            merge_requested,
            merge_succeeded,
            deploy_required,
        })
    }
}

pub(crate) struct SubmitArgs {
    pub(crate) repo: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) request: String,
    pub(crate) task_type: Option<TaskType>,
    pub(crate) agenda_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) task_status: Option<String>,
    pub(crate) task_created_at: Option<String>,
    pub(crate) task_updated_at: Option<String>,
    pub(crate) role_mode: String,
    pub(crate) github_owner: Option<String>,
    pub(crate) github_repo: Option<String>,
    pub(crate) github_issue_number: Option<u64>,
    pub(crate) github_issue_url: Option<String>,
    pub(crate) github_project_mode: GitHubProjectMode,
    pub(crate) github_project_mode_explicit: bool,
    pub(crate) github_project_id: Option<String>,
    pub(crate) github_project_url: Option<String>,
    pub(crate) github_project_item_id: Option<String>,
    pub(crate) github_project_status: Option<String>,
    pub(crate) github_project_owner_kind: Option<String>,
    pub(crate) github_project_owner: Option<String>,
    pub(crate) github_project_number: Option<u64>,
    pub(crate) github_project_status_field_id: Option<String>,
    pub(crate) github_project_status_field_name: Option<String>,
    pub(crate) github_project_status_option_id: Option<String>,
    pub(crate) github_project_status_option_name: Option<String>,
    pub(crate) allow_github_mutation: bool,
}

impl SubmitArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
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
    pub(crate) fn has_upstream_provenance(&self) -> bool {
        self.task_status.is_some()
            || self.task_created_at.is_some()
            || self.task_updated_at.is_some()
    }

    pub(crate) fn upstream_provenance_markdown(&self) -> Option<String> {
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

pub(crate) struct WatchArgs {
    pub(crate) repo: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) tasks_path: PathBuf,
    pub(crate) once: bool,
}

impl WatchArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
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

pub(crate) struct FinalizeArgs {
    pub(crate) repo: PathBuf,
    pub(crate) state: Option<PathBuf>,
    pub(crate) agenda_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) github_issue_number: Option<u64>,
    pub(crate) allow_mutation: bool,
}

impl FinalizeArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
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

pub(crate) struct FinalizeApprovedArgs {
    pub(crate) repo: Option<PathBuf>,
    pub(crate) state: Option<PathBuf>,
    pub(crate) tasks_path: PathBuf,
    pub(crate) task_id: String,
    pub(crate) json: bool,
}

impl FinalizeApprovedArgs {
    pub(crate) fn parse(args: &[String]) -> CanopusResult<Self> {
        let mut repo = env_non_empty("CANOPUS_REPO")
            .or_else(|| env_non_empty("CANOPUS_REPO_PATH"))
            .map(PathBuf::from);
        let mut state: Option<PathBuf> = env_non_empty("CANOPUS_STATE")
            .or_else(|| env_non_empty("CANOPUS_STATE_PATH"))
            .map(PathBuf::from);
        let mut tasks_path: Option<PathBuf> = None;
        let mut task_id = None;
        let mut json = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--repo" => {
                    index += 1;
                    repo = Some(PathBuf::from(required_value(args, index, "--repo")?));
                }
                "--state" => {
                    index += 1;
                    state = Some(PathBuf::from(required_value(args, index, "--state")?));
                }
                "--tasks" => {
                    index += 1;
                    tasks_path = Some(PathBuf::from(required_value(args, index, "--tasks")?));
                }
                "--task-id" => {
                    index += 1;
                    task_id = Some(required_value(args, index, "--task-id")?.to_string());
                }
                "--json" => json = true,
                value => {
                    return Err(CanopusError::InvalidInput(format!(
                        "unknown finalize-approved argument: {value}"
                    )));
                }
            }
            index += 1;
        }

        Ok(Self {
            repo,
            state,
            tasks_path: tasks_path.ok_or_else(|| {
                CanopusError::InvalidInput("finalize-approved requires --tasks".to_string())
            })?,
            task_id: task_id.ok_or_else(|| {
                CanopusError::InvalidInput("finalize-approved requires --task-id".to_string())
            })?,
            json,
        })
    }
}

pub(crate) fn required_value<'a>(
    args: &'a [String],
    index: usize,
    name: &str,
) -> CanopusResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CanopusError::InvalidInput(format!("{name} requires a value")))
}

pub(crate) fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

pub(crate) fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn env_u64(name: &str) -> CanopusResult<Option<u64>> {
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

pub(crate) fn parse_task_type(value: &str) -> CanopusResult<TaskType> {
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

#[cfg(test)]
mod derive_state_for_run_tests {
    use super::*;

    fn unique_tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()))
    }

    #[test]
    fn returns_env_fallback_when_payload_repo_absent() {
        let env_state = unique_tmp("derive-state-env-fallback");
        let _ = fs::remove_dir_all(&env_state);
        let (state, source) = derive_state_with_source(None, &env_state);
        assert_eq!(state, env_state);
        assert_eq!(source, StateSource::EnvFallback);
        assert_eq!(source.as_str(), "env_fallback");
    }

    #[test]
    fn returns_payload_repo_canopus_when_directory_writable() {
        let payload_repo = unique_tmp("derive-state-payload-writable");
        let env_state = unique_tmp("derive-state-payload-writable-env");
        let _ = fs::remove_dir_all(&payload_repo);
        let _ = fs::remove_dir_all(&env_state);
        fs::create_dir_all(&payload_repo).unwrap();

        let (state, source) = derive_state_with_source(Some(&payload_repo), &env_state);

        assert_eq!(state, payload_repo.join(".canopus"));
        assert_eq!(source, StateSource::PayloadRepo);
        assert_eq!(source.as_str(), "payload_repo");
        assert!(state.is_dir(), "derive must create candidate directory");
        assert!(
            !state.join(".write-probe").exists(),
            "probe artifact must be cleaned up"
        );
        let _ = fs::remove_dir_all(&payload_repo);
    }

    #[test]
    fn returns_env_fallback_when_payload_repo_path_is_a_file() {
        // probe failure path: parent is a regular file → create_dir_all under it fails.
        let parent = unique_tmp("derive-state-not-a-dir");
        let _ = fs::remove_dir_all(&parent);
        fs::create_dir_all(parent.parent().unwrap()).unwrap();
        let payload_repo = parent.join("file-not-dir");
        if let Some(p) = payload_repo.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(&payload_repo, b"i am a file, not a directory\n").unwrap();
        let env_state = unique_tmp("derive-state-not-a-dir-env");
        let _ = fs::remove_dir_all(&env_state);

        let (state, source) = derive_state_with_source(Some(&payload_repo), &env_state);

        assert_eq!(
            state, env_state,
            "must fall back to env_state on probe fail"
        );
        assert_eq!(source, StateSource::EnvFallback);
        let _ = fs::remove_file(&payload_repo);
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn self_hosting_payload_equals_env_state_returns_same_canopus_path() {
        // plan §8.2: when payload_repo points at the same tree the env_state
        // already lives under, derivation must yield the same .canopus path
        // (no double-derivation, no migration).
        let repo = unique_tmp("derive-state-self-hosting");
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(&repo).unwrap();
        let env_state = repo.join(".canopus");

        let (state, source) = derive_state_with_source(Some(&repo), &env_state);

        assert_eq!(state, env_state);
        assert_eq!(source, StateSource::PayloadRepo);
        let _ = fs::remove_dir_all(&repo);
    }

    #[test]
    fn public_helper_returns_pathbuf_only() {
        // PR-A spec signature: pub fn derive_state_for_run(...) -> PathBuf.
        let payload_repo = unique_tmp("derive-state-public-signature");
        let _ = fs::remove_dir_all(&payload_repo);
        fs::create_dir_all(&payload_repo).unwrap();
        let env_state = unique_tmp("derive-state-public-signature-env");

        let resolved: PathBuf = derive_state_for_run(Some(&payload_repo), &env_state);
        assert_eq!(resolved, payload_repo.join(".canopus"));
        let _ = fs::remove_dir_all(&payload_repo);
    }
}
