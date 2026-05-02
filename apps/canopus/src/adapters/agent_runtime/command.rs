use crate::core::{AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusError, CanopusResult};
use crate::ports::{AgentContext, AgentRuntime};
use async_trait::async_trait;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct CommandAgentRuntime {
    command: Vec<String>,
}

impl CommandAgentRuntime {
    pub fn from_env() -> Option<Self> {
        let command = std::env::var("CANOPUS_AGENT_COMMAND").ok()?;
        Self::new(command).ok()
    }

    pub fn new(command: impl AsRef<str>) -> CanopusResult<Self> {
        let command = shell_words(command.as_ref());
        if command.is_empty() {
            return Err(CanopusError::InvalidInput(
                "CANOPUS_AGENT_COMMAND must not be empty".to_string(),
            ));
        }
        Ok(Self { command })
    }
}

#[async_trait]
impl AgentRuntime for CommandAgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        context: &AgentContext,
        prior_artifacts: &[Artifact],
    ) -> CanopusResult<AgentRunResult> {
        let mut cmd = Command::new(&self.command[0]);
        cmd.args(&self.command[1..])
            .current_dir(&context.repo_path)
            .env("CANOPUS_TASK_ID", &task.id)
            .env("CANOPUS_AGENDA_ID", &task.agenda_id)
            .env("CANOPUS_ROLE", task.role.as_str())
            .env("CANOPUS_ROLE_MODE", &task.role_mode)
            .env("CANOPUS_PROMPT", &task.prompt)
            .env(
                "CANOPUS_PRIOR_ARTIFACT_COUNT",
                prior_artifacts.len().to_string(),
            );

        let output = cmd.output()?;
        let status = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let content = format!(
            "# Command runtime\n\ncommand: `{}`\nstatus: {}\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
            self.command.join(" "),
            status,
            stdout,
            stderr
        );

        if !output.status.success() {
            return Err(CanopusError::Runtime(content));
        }

        Ok(AgentRunResult {
            task_id: task.id.clone(),
            summary: "command runtime completed".to_string(),
            artifacts: vec![Artifact {
                task_id: task.id.clone(),
                kind: ArtifactKind::RuntimeLog,
                content,
            }],
        })
    }
}

fn shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (None, c) => current.push(c),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }
    words
}
