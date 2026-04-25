use crate::core::{AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime};
use std::fs;

#[derive(Debug, Clone, Copy)]
pub struct MockAgentRuntime;

impl AgentRuntime for MockAgentRuntime {
    fn run(&self, task: &AgentTask, context: &AgentContext) -> CanopusResult<AgentRunResult> {
        match task.role {
            AgentRole::Planner => Ok(AgentRunResult {
                task_id: task.id.clone(),
                summary: "mock planner completed".to_string(),
                artifacts: vec![Artifact {
                    task_id: task.id.clone(),
                    kind: ArtifactKind::Plan,
                    content: format!("# Mock plan\n\n{}\n", task.prompt),
                }],
            }),
            AgentRole::Coder => {
                let output = context.repo_path.join("canopus-mock-output.txt");
                fs::write(&output, format!("{}\n", task.prompt))?;
                Ok(AgentRunResult {
                    task_id: task.id.clone(),
                    summary: "mock coder wrote canopus-mock-output.txt".to_string(),
                    artifacts: vec![Artifact {
                        task_id: task.id.clone(),
                        kind: ArtifactKind::RuntimeLog,
                        content: format!("Wrote {}\n", output.display()),
                    }],
                })
            }
            AgentRole::Reviewer => Ok(AgentRunResult {
                task_id: task.id.clone(),
                summary: "mock reviewer completed".to_string(),
                artifacts: vec![Artifact {
                    task_id: task.id.clone(),
                    kind: ArtifactKind::Review,
                    content: "# Mock review\n\nNo issues found in mock runtime.\n".to_string(),
                }],
            }),
        }
    }
}
