use crate::core::error::{CanopusError, CanopusResult};
use crate::core::run_identity::derive_run_identity;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agenda {
    pub id: String,
    pub request: String,
    pub source: AgendaSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgendaSource {
    Cli,
    GitHubIssue {
        owner: String,
        repo: String,
        number: u64,
    },
    GitHubProject {
        project_url: String,
        item_id: String,
    },
}

impl AgendaSource {
    pub fn kind(&self) -> &'static str {
        match self {
            AgendaSource::Cli => "cli",
            AgendaSource::GitHubIssue { .. } => "github_issue",
            AgendaSource::GitHubProject { .. } => "github_project",
        }
    }
}

impl Agenda {
    pub fn new_with_id(id: impl Into<String>, request: impl Into<String>) -> CanopusResult<Self> {
        Self::new_with_source(id, request, AgendaSource::Cli)
    }

    pub fn new_with_source(
        id: impl Into<String>,
        request: impl Into<String>,
        source: AgendaSource,
    ) -> CanopusResult<Self> {
        let request = request.into();
        if request.trim().is_empty() {
            return Err(CanopusError::InvalidInput(
                "request must not be empty".to_string(),
            ));
        }

        Ok(Self {
            id: id.into(),
            request: request.trim().to_string(),
            source,
        })
    }

    /// Construct an agenda whose id is deterministically derived from a GitHub Issue identity.
    ///
    /// The same `(owner, repo, number)` always produces the same id (run through
    /// [`derive_run_identity`] for consistent sanitisation), so re-processing the
    /// same Issue stays idempotent — the V2 GitHub-Issue-as-ledger bridge.
    pub fn from_github_issue(
        owner: impl Into<String>,
        repo: impl Into<String>,
        number: u64,
        request: impl Into<String>,
    ) -> CanopusResult<Self> {
        let owner = owner.into();
        let repo = repo.into();
        let id = deterministic_agenda_id_for_github_issue(&owner, &repo, number)?;
        Self::new_with_source(
            id,
            request,
            AgendaSource::GitHubIssue {
                owner,
                repo,
                number,
            },
        )
    }

    /// Construct an agenda whose id is deterministically derived from a GitHub Project v2 item.
    pub fn from_github_project(
        project_url: impl Into<String>,
        item_id: impl Into<String>,
        request: impl Into<String>,
    ) -> CanopusResult<Self> {
        let project_url = project_url.into();
        let item_id = item_id.into();
        let id = deterministic_agenda_id_for_github_project(&project_url, &item_id)?;
        Self::new_with_source(
            id,
            request,
            AgendaSource::GitHubProject {
                project_url,
                item_id,
            },
        )
    }
}

/// Build a deterministic agenda id from a GitHub Issue identity.
///
/// Reuses [`derive_run_identity`] so the result follows the same sanitisation rules
/// every other run identifier in canopus uses (lowercase ASCII alphanumerics joined
/// by dashes). Exposed publicly so external callers (Europa bot, future adapters)
/// can produce ids compatible with canopus' run/agenda lookup.
pub fn deterministic_agenda_id_for_github_issue(
    owner: &str,
    repo: &str,
    number: u64,
) -> CanopusResult<String> {
    let raw = format!("gh-{owner}-{repo}-{number}");
    derive_run_identity(Some(&raw), None)
}

/// Build a deterministic agenda id from a GitHub Project v2 item identity.
pub fn deterministic_agenda_id_for_github_project(
    project_url: &str,
    item_id: &str,
) -> CanopusResult<String> {
    let raw = format!("ghp-{project_url}-{item_id}");
    derive_run_identity(Some(&raw), None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRole {
    Planner,
    Coder,
    Reviewer,
    Custom(String),
}

impl AgentRole {
    pub fn as_str(&self) -> &str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Coder => "coder",
            AgentRole::Reviewer => "reviewer",
            AgentRole::Custom(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTask {
    pub id: String,
    pub agenda_id: String,
    pub role: AgentRole,
    pub prompt: String,
    pub role_mode: String,
    pub source_task_id: Option<String>,
    pub github_issue: Option<GitHubIssueMetadata>,
    pub github_project: Option<GitHubProjectMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubIssueMetadata {
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub number: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GitHubProjectMode {
    #[default]
    DryRunOffline,
    ValidateReadOnly,
    MutateLive,
}

impl GitHubProjectMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DryRunOffline => "dry-run-offline",
            Self::ValidateReadOnly => "validate-read-only",
            Self::MutateLive => "mutate-live",
        }
    }

    pub fn parse(value: &str) -> CanopusResult<Self> {
        match value {
            "dry-run-offline" => Ok(Self::DryRunOffline),
            "validate-read-only" => Ok(Self::ValidateReadOnly),
            "mutate-live" => Ok(Self::MutateLive),
            _ => Err(CanopusError::InvalidInput(format!(
                "unsupported GitHub Project mode `{value}` (expected dry-run-offline, validate-read-only, or mutate-live)"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubProjectMetadata {
    pub id: Option<String>,
    pub url: Option<String>,
    pub item_id: Option<String>,
    pub status: Option<String>,
    pub owner_kind: Option<String>,
    pub owner: Option<String>,
    pub number: Option<u64>,
    pub status_field_id: Option<String>,
    pub status_field_name: Option<String>,
    pub status_option_id: Option<String>,
    pub status_option_name: Option<String>,
    pub mode: Option<GitHubProjectMode>,
}

impl GitHubProjectMetadata {
    pub fn is_empty(&self) -> bool {
        self.id.is_none()
            && self.url.is_none()
            && self.item_id.is_none()
            && self.status.is_none()
            && self.owner_kind.is_none()
            && self.owner.is_none()
            && self.number.is_none()
            && self.status_field_id.is_none()
            && self.status_field_name.is_none()
            && self.status_option_id.is_none()
            && self.status_option_name.is_none()
            && self.mode.is_none()
    }
}

impl AgentTask {
    pub fn for_agenda(id: impl Into<String>, agenda: &Agenda, role: AgentRole) -> Self {
        Self::for_agenda_with_metadata(id, agenda, role, "standard", None, None)
    }

    pub fn for_agenda_with_metadata(
        id: impl Into<String>,
        agenda: &Agenda,
        role: AgentRole,
        role_mode: impl Into<String>,
        source_task_id: Option<String>,
        github_issue: Option<GitHubIssueMetadata>,
    ) -> Self {
        Self::for_agenda_with_all_metadata(
            id,
            agenda,
            role,
            role_mode,
            source_task_id,
            github_issue,
            None,
        )
    }

    pub fn for_agenda_with_all_metadata(
        id: impl Into<String>,
        agenda: &Agenda,
        role: AgentRole,
        role_mode: impl Into<String>,
        source_task_id: Option<String>,
        github_issue: Option<GitHubIssueMetadata>,
        github_project: Option<GitHubProjectMetadata>,
    ) -> Self {
        let role_mode = role_mode.into();
        let mut prompt = format!("Agenda {}: {}", agenda.id, agenda.request);
        if let Some(task_id) = &source_task_id {
            prompt.push_str(&format!("\nSource task: {task_id}"));
        }
        if !role_mode.trim().is_empty() {
            prompt.push_str(&format!("\nRole mode: {role_mode}"));
        }
        if let Some(issue) = &github_issue {
            if let Some(number) = issue.number {
                prompt.push_str(&format!("\nGitHub issue: #{number}"));
            }
            if let Some(url) = &issue.url {
                prompt.push_str(&format!("\nGitHub issue URL: {url}"));
            }
        }
        if let Some(project) = &github_project {
            if let Some(id) = &project.id {
                prompt.push_str(&format!("\nGitHub project ID: {id}"));
            }
            if let Some(url) = &project.url {
                prompt.push_str(&format!("\nGitHub project URL: {url}"));
            }
            if let Some(item_id) = &project.item_id {
                prompt.push_str(&format!("\nGitHub project item ID: {item_id}"));
            }
            if let Some(status) = &project.status {
                prompt.push_str(&format!("\nGitHub project status: {status}"));
            }
        }

        Self {
            id: id.into(),
            agenda_id: agenda.id.clone(),
            role,
            prompt,
            role_mode,
            source_task_id,
            github_issue,
            github_project,
        }
    }

    pub fn to_backend_payload(&self) -> String {
        let mut payload = format!(
            "agenda_id={}\nrole={}\nrole_mode={}\nprompt={}\n",
            self.agenda_id,
            self.role.as_str(),
            self.role_mode,
            self.prompt
        );
        if let Some(task_id) = &self.source_task_id {
            payload.push_str(&format!("task_id={task_id}\n"));
        }
        if let Some(issue) = &self.github_issue {
            if let Some(owner) = &issue.owner {
                payload.push_str(&format!("github_owner={owner}\n"));
            }
            if let Some(repo) = &issue.repo {
                payload.push_str(&format!("github_repo={repo}\n"));
            }
            if let Some(number) = issue.number {
                payload.push_str(&format!("github_issue_number={number}\n"));
            }
            if let Some(url) = &issue.url {
                payload.push_str(&format!("github_issue_url={url}\n"));
            }
        }
        if let Some(project) = &self.github_project {
            if let Some(id) = &project.id {
                payload.push_str(&format!("github_project_id={id}\n"));
            }
            if let Some(url) = &project.url {
                payload.push_str(&format!("github_project_url={url}\n"));
            }
            if let Some(item_id) = &project.item_id {
                payload.push_str(&format!("github_project_item_id={item_id}\n"));
            }
            if let Some(status) = &project.status {
                payload.push_str(&format!("github_project_status={status}\n"));
            }
            if let Some(owner_kind) = &project.owner_kind {
                payload.push_str(&format!("github_project_owner_kind={owner_kind}\n"));
            }
            if let Some(owner) = &project.owner {
                payload.push_str(&format!("github_project_owner={owner}\n"));
            }
            if let Some(number) = project.number {
                payload.push_str(&format!("github_project_number={number}\n"));
            }
            if let Some(field_id) = &project.status_field_id {
                payload.push_str(&format!("github_project_status_field_id={field_id}\n"));
            }
            if let Some(field_name) = &project.status_field_name {
                payload.push_str(&format!("github_project_status_field_name={field_name}\n"));
            }
            if let Some(option_id) = &project.status_option_id {
                payload.push_str(&format!("github_project_status_option_id={option_id}\n"));
            }
            if let Some(option_name) = &project.status_option_name {
                payload.push_str(&format!(
                    "github_project_status_option_name={option_name}\n"
                ));
            }
            if let Some(mode) = project.mode {
                payload.push_str(&format!("github_project_mode={}\n", mode.as_str()));
            }
        }
        payload
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactKind {
    Plan,
    Diff,
    TestResult,
    Review,
    RuntimeLog,
}

impl ArtifactKind {
    pub fn file_name(&self) -> &'static str {
        match self {
            ArtifactKind::Plan => "plan.md",
            ArtifactKind::Diff => "diff.md",
            ArtifactKind::TestResult => "test-result.md",
            ArtifactKind::Review => "review.md",
            ArtifactKind::RuntimeLog => "runtime-log.md",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub task_id: String,
    pub kind: ArtifactKind,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunResult {
    pub task_id: String,
    pub summary: String,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StageRecord {
    pub name: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_secs: u64,
    pub status: String,
    pub artifacts: Vec<String>,
}
