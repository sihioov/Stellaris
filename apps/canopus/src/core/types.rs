use crate::core::error::{CanopusError, CanopusResult};

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
            return Err(CanopusError::InvalidInput("request must not be empty".to_string()));
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
}

impl AgentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentRole::Planner => "planner",
            AgentRole::Coder => "coder",
            AgentRole::Reviewer => "reviewer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTask {
    pub id: String,
    pub agenda_id: String,
    pub role: AgentRole,
    pub prompt: String,
}

impl AgentTask {
    pub fn for_agenda(id: impl Into<String>, agenda: &Agenda, role: AgentRole) -> Self {
        Self {
            id: id.into(),
            agenda_id: agenda.id.clone(),
            role,
            prompt: format!("Agenda {}: {}", agenda.id, agenda.request),
        }
    }

    pub fn to_backend_payload(&self) -> String {
        format!(
            "agenda_id={}\nrole={}\nprompt={}\n",
            self.agenda_id,
            self.role.as_str(),
            self.prompt
        )
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
