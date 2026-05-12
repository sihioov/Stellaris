use crate::core::types::AgentRole;
use dysonsphere::message::TaskType;

#[derive(Debug, Clone, PartialEq)]
pub enum Pipeline {
    DevMode,       // Analyst → Planner → Coder → Reviewer
    PlannerOnly,   // Planner
    ReviewerOnly,  // Reviewer
    BugFix,        // Analyzer → Coder → Tester
    SecurityAudit, // SecurityAuditor → Coder → Reviewer
    TestWriter,    // Analyzer → TestWriter
    UXImprovement, // Researcher → Planner → Reviewer
}

impl Pipeline {
    pub fn from_task_type(task_type: &TaskType) -> Self {
        match task_type {
            TaskType::Bug => Pipeline::BugFix,
            TaskType::Security => Pipeline::SecurityAudit,
            TaskType::TestCoverage => Pipeline::TestWriter,
            TaskType::UXImprovement => Pipeline::UXImprovement,
            _ => Pipeline::DevMode,
        }
    }

    /// 이 파이프라인의 에이전트 역할 순서 반환
    pub fn agent_roles(&self) -> Vec<AgentRole> {
        match self {
            Pipeline::PlannerOnly => vec![AgentRole::Planner],
            Pipeline::ReviewerOnly => vec![AgentRole::Reviewer],
            Pipeline::DevMode => vec![
                AgentRole::Custom("analyst".to_string()),
                AgentRole::Planner,
                AgentRole::Coder,
                AgentRole::Reviewer,
            ],
            Pipeline::BugFix => vec![
                AgentRole::Custom("analyzer".to_string()),
                AgentRole::Coder,
                AgentRole::Custom("tester".to_string()),
            ],
            Pipeline::SecurityAudit => vec![
                AgentRole::Custom("security_auditor".to_string()),
                AgentRole::Coder,
                AgentRole::Reviewer,
            ],
            Pipeline::TestWriter => vec![
                AgentRole::Custom("analyzer".to_string()),
                AgentRole::Custom("test_writer".to_string()),
            ],
            Pipeline::UXImprovement => vec![
                AgentRole::Custom("researcher".to_string()),
                AgentRole::Planner,
                AgentRole::Reviewer,
            ],
        }
    }
}
