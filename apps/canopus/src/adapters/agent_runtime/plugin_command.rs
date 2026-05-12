use crate::core::{
    AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusError, CanopusResult,
    RuntimeCapability,
};
use crate::ports::{AgentContext, AgentRuntime};
use async_trait::async_trait;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct PluginCommandAgentRuntime {
    backend_name: String,
    capability: RuntimeCapability,
    argv: Vec<String>,
    env_allowlist: Vec<String>,
    timeout_seconds: Option<u64>,
}

impl PluginCommandAgentRuntime {
    pub fn new(
        backend_name: impl Into<String>,
        capability: RuntimeCapability,
        argv: Vec<String>,
        env_allowlist: Vec<String>,
        timeout_seconds: Option<u64>,
    ) -> CanopusResult<Self> {
        if argv.is_empty() {
            return Err(CanopusError::InvalidInput(
                "plugin command runtime requires non-empty argv".to_string(),
            ));
        }
        Ok(Self {
            backend_name: backend_name.into(),
            capability,
            argv,
            env_allowlist,
            timeout_seconds,
        })
    }
}

#[async_trait]
impl AgentRuntime for PluginCommandAgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        context: &AgentContext,
        prior_artifacts: &[Artifact],
    ) -> CanopusResult<AgentRunResult> {
        let mut cmd = Command::new(&self.argv[0]);
        cmd.args(&self.argv[1..])
            .current_dir(&context.repo_path)
            .env_clear()
            .env("CANOPUS_TASK_ID", &task.id)
            .env("CANOPUS_AGENDA_ID", &task.agenda_id)
            .env("CANOPUS_ROLE", task.role.as_str())
            .env("CANOPUS_ROLE_MODE", &task.role_mode)
            .env("CANOPUS_CAPABILITY", self.capability.as_str())
            .env("CANOPUS_BACKEND", &self.backend_name)
            .env("CANOPUS_PROMPT", &task.prompt)
            .env(
                "CANOPUS_PRIOR_ARTIFACT_COUNT",
                prior_artifacts.len().to_string(),
            );
        for key in &self.env_allowlist {
            if !is_allowed_env_key(key) {
                continue;
            }
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }

        let output = output_with_timeout(cmd, self.timeout_seconds)?;
        let status = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let timeout = self
            .timeout_seconds
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());
        let content = format!(
            "# Plugin command runtime\n\nbackend: {}\ncapability: {}\nargv: `{}`\ntimeout_seconds: {}\nstatus: {}\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
            self.backend_name,
            self.capability.as_str(),
            self.argv.join(" "),
            timeout,
            status,
            stdout,
            stderr
        );

        if !output.status.success() {
            return Err(CanopusError::Runtime(content));
        }

        Ok(AgentRunResult {
            task_id: task.id.clone(),
            summary: format!(
                "plugin backend '{}' completed {}",
                self.backend_name,
                self.capability.as_str()
            ),
            artifacts: vec![Artifact {
                task_id: task.id.clone(),
                kind: ArtifactKind::RuntimeLog,
                content,
            }],
            message_log: Vec::new(),
            token_usage: None,
        })
    }
}

fn output_with_timeout(mut cmd: Command, timeout_seconds: Option<u64>) -> CanopusResult<Output> {
    let Some(timeout_seconds) = timeout_seconds else {
        return Ok(cmd.output()?);
    };
    let timeout = Duration::from_secs(timeout_seconds);
    let start = Instant::now();
    let mut child = cmd.spawn()?;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output()?;
            return Err(CanopusError::Runtime(format!(
                "plugin command runtime timed out after {timeout_seconds}s\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn is_allowed_env_key(key: &str) -> bool {
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
        return false;
    }
    let upper = key.to_ascii_uppercase();
    if upper.contains("TOKEN")
        || upper.contains("SECRET")
        || upper.contains("PASSWORD")
        || upper.contains("PRIVATE_KEY")
    {
        return false;
    }
    !matches!(
        upper.as_str(),
        "CANOPUS_ENABLE_LIVE_MUTATIONS"
            | "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION"
            | "CANOPUS_ALLOW_GITHUB_PR_MUTATION"
            | "CANOPUS_ALLOW_GITHUB_MERGE"
            | "CANOPUS_ALLOW_DEPLOY"
            | "GITHUB_TOKEN"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_and_secret_env_names_are_denied() {
        assert!(!is_allowed_env_key("GITHUB_TOKEN"));
        assert!(!is_allowed_env_key("CANOPUS_ENABLE_LIVE_MUTATIONS"));
        assert!(!is_allowed_env_key("MY_SECRET"));
        assert!(is_allowed_env_key("SAFE_HINT"));
    }
}
