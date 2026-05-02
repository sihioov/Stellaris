use crate::core::error::{CanopusError, CanopusResult};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agenda {
    pub id: String,
    pub request: String,
    pub source: String,
}

impl Agenda {
    pub fn new_with_id(id: impl Into<String>, request: impl Into<String>) -> CanopusResult<Self> {
        let request = request.into();
        if request.trim().is_empty() {
            return Err(CanopusError::InvalidInput(
                "request must not be empty".to_string(),
            ));
        }

        Ok(Self {
            id: id.into(),
            request: request.trim().to_string(),
            source: "cli".to_string(),
        })
    }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubIssueMetadata {
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub number: Option<u64>,
    pub url: Option<String>,
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

        Self {
            id: id.into(),
            agenda_id: agenda.id.clone(),
            role,
            prompt,
            role_mode,
            source_task_id,
            github_issue,
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
