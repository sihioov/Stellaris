use crate::core::{AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime};
use async_trait::async_trait;
use std::fs;

#[derive(Debug, Clone, Copy)]
pub struct MockAgentRuntime;

#[async_trait]
impl AgentRuntime for MockAgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        context: &AgentContext,
        prior_artifacts: &[Artifact],
    ) -> CanopusResult<AgentRunResult> {
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
                let context_summary = prior_artifacts
                    .iter()
                    .map(|a| format!("## {}\n{}", a.kind.file_name(), a.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let file_content = if context_summary.is_empty() {
                    format!("{}\n", task.prompt)
                } else {
                    format!(
                        "{}\n\n# Prior Context\n\n{}\n",
                        task.prompt, context_summary
                    )
                };
                let output = context.repo_path.join("canopus-mock-output.txt");
                fs::write(&output, &file_content)?;
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
            AgentRole::Reviewer => {
                let context_summary = prior_artifacts
                    .iter()
                    .map(|a| format!("## {}\n{}", a.kind.file_name(), a.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let review_content = if context_summary.is_empty() {
                    "# Mock review\n\nNo issues found in mock runtime.\n".to_string()
                } else {
                    format!(
                        "# Mock review\n\nNo issues found in mock runtime.\n\n# Prior Context\n\n{}\n",
                        context_summary
                    )
                };
                Ok(AgentRunResult {
                    task_id: task.id.clone(),
                    summary: "mock reviewer completed".to_string(),
                    artifacts: vec![Artifact {
                        task_id: task.id.clone(),
                        kind: ArtifactKind::Review,
                        content: review_content,
                    }],
                })
            }
            AgentRole::Custom(ref name) => Ok(AgentRunResult {
                task_id: task.id.clone(),
                summary: format!("mock {} completed", name),
                artifacts: vec![Artifact {
                    task_id: task.id.clone(),
                    kind: ArtifactKind::RuntimeLog,
                    content: format!("# Mock {}\n\n{}\n", name, task.prompt),
                }],
            }),
        }
    }
}
