use crate::core::{
    AgentMessage, AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind, CanopusError,
    CanopusResult, TokenUsage,
};
use crate::ports::{AgentContext, AgentRuntime};
use async_trait::async_trait;
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct CodexAgentRuntime {
    command: Vec<String>,
    sandbox: String,
    approval_policy: String,
    model: Option<String>,
    profile: Option<String>,
}

impl CodexAgentRuntime {
    pub fn from_env() -> CanopusResult<Self> {
        let command = std::env::var("CANOPUS_CODEX_COMMAND")
            .ok()
            .map(|value| shell_words(&value))
            .filter(|parts| !parts.is_empty())
            .unwrap_or_else(|| vec!["codex".to_string()]);
        let mut runtime = Self::new(command)?;
        if let Ok(model) = std::env::var("CANOPUS_CODEX_MODEL") {
            if !model.trim().is_empty() {
                runtime.model = Some(model);
            }
        }
        if let Ok(profile) = std::env::var("CANOPUS_CODEX_PROFILE") {
            if !profile.trim().is_empty() {
                runtime.profile = Some(profile);
            }
        }
        if let Ok(sandbox) = std::env::var("CANOPUS_CODEX_SANDBOX") {
            if !sandbox.trim().is_empty() {
                runtime.sandbox = sandbox;
            }
        }
        if let Ok(approval_policy) = std::env::var("CANOPUS_CODEX_APPROVAL_POLICY") {
            if !approval_policy.trim().is_empty() {
                runtime.approval_policy = approval_policy;
            }
        }
        Ok(runtime)
    }

    pub fn new(command: Vec<String>) -> CanopusResult<Self> {
        if command.is_empty() {
            return Err(CanopusError::InvalidInput(
                "CANOPUS_CODEX_COMMAND must not be empty".to_string(),
            ));
        }
        Ok(Self {
            command,
            sandbox: "workspace-write".to_string(),
            approval_policy: "never".to_string(),
            model: None,
            profile: None,
        })
    }
}

#[async_trait]
impl AgentRuntime for CodexAgentRuntime {
    async fn run(
        &self,
        task: &AgentTask,
        context: &AgentContext,
        prior_artifacts: &[Artifact],
    ) -> CanopusResult<AgentRunResult> {
        let prompt = codex_prompt(task, prior_artifacts);
        let last_message_path = last_message_path(task)?;
        if let Some(parent) = last_message_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut cmd = Command::new(&self.command[0]);
        cmd.args(&self.command[1..])
            .arg("--ask-for-approval")
            .arg(&self.approval_policy)
            .arg("exec")
            .arg("--cd")
            .arg(&context.repo_path)
            .arg("--sandbox")
            .arg(sandbox_for_role(&task.role, &self.sandbox))
            .arg("--output-last-message")
            .arg(&last_message_path);
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(profile) = &self.profile {
            cmd.arg("--profile").arg(profile);
        }
        cmd.arg("-")
            .current_dir(&context.repo_path)
            .env("CANOPUS_TASK_ID", &task.id)
            .env("CANOPUS_AGENDA_ID", &task.agenda_id)
            .env("CANOPUS_ROLE", task.role.as_str())
            .env("CANOPUS_ROLE_MODE", &task.role_mode)
            .env("CANOPUS_PROMPT", &task.prompt)
            .env(
                "CANOPUS_PRIOR_ARTIFACT_COUNT",
                prior_artifacts.len().to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| CanopusError::Runtime("failed to open codex stdin".to_string()))?
            .write_all(prompt.as_bytes())?;
        let output = child.wait_with_output()?;
        let status = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let final_message = fs::read_to_string(&last_message_path)
            .ok()
            .filter(|message| !message.trim().is_empty())
            .unwrap_or_else(|| stdout.clone());
        let content = runtime_log(&self.command, status, &stdout, &stderr, &final_message);

        let _ = fs::remove_file(&last_message_path);

        if !output.status.success() {
            return Err(CanopusError::Runtime(content));
        }

        let now = Utc::now();
        Ok(AgentRunResult {
            task_id: task.id.clone(),
            summary: summary_for_role(&task.role),
            artifacts: vec![Artifact {
                task_id: task.id.clone(),
                kind: artifact_kind_for_role(&task.role),
                content: artifact_content_for_role(&task.role, &content, &final_message),
            }],
            message_log: vec![
                AgentMessage {
                    role: "user".to_string(),
                    content: prompt,
                    created_at: now,
                },
                AgentMessage {
                    role: "assistant".to_string(),
                    content: final_message,
                    created_at: Utc::now(),
                },
            ],
            token_usage: extract_token_usage(&stdout),
        })
    }
}

fn extract_token_usage(stdout: &str) -> Option<TokenUsage> {
    for line in stdout.lines() {
        if !line.contains("\"usage\"") {
            continue;
        }
        // Try to find a JSON object in the line containing a usage block.
        if let Some(start) = line.find('{') {
            let fragment = &line[start..];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(fragment) {
                // Responses API: {"usage": {"input_tokens": N, "output_tokens": N}}
                if let (Some(input), Some(output)) = (
                    v["usage"]["input_tokens"].as_u64(),
                    v["usage"]["output_tokens"].as_u64(),
                ) {
                    return Some(TokenUsage { input_tokens: input, output_tokens: output });
                }
                // Chat Completions: {"usage": {"prompt_tokens": N, "completion_tokens": N}}
                if let (Some(prompt), Some(completion)) = (
                    v["usage"]["prompt_tokens"].as_u64(),
                    v["usage"]["completion_tokens"].as_u64(),
                ) {
                    return Some(TokenUsage { input_tokens: prompt, output_tokens: completion });
                }
                // Nested: search one level deeper for a usage object with total_tokens
                if let Some(total) = v["usage"]["total_tokens"].as_u64() {
                    let input = v["usage"]["input_tokens"]
                        .as_u64()
                        .or_else(|| v["usage"]["prompt_tokens"].as_u64())
                        .unwrap_or(0);
                    return Some(TokenUsage { input_tokens: input, output_tokens: total.saturating_sub(input) });
                }
            }
        }
    }
    None
}

fn codex_prompt(task: &AgentTask, prior_artifacts: &[Artifact]) -> String {
    let role_instruction = match &task.role {
        AgentRole::Planner => {
            "Act as the Canopus planner. Produce a concise implementation plan and do not edit files."
        }
        AgentRole::Coder => {
            "Act as the Canopus coder. Implement the requested local repository change. Keep changes scoped, reviewable, and testable."
        }
        AgentRole::Reviewer => {
            "Act as the Canopus reviewer. Review the current repository diff and prior artifacts. Do not edit files."
        }
        AgentRole::Custom(name) => match name.as_str() {
            "analyzer" => "Act as the Canopus analyzer. Diagnose the task and report concrete findings. Do not edit files.",
            "analyst" => "Act as the Canopus analyst. Analyze the current request and repository context. Do not edit files.",
            "tester" => "Act as the Canopus tester. Run or describe the smallest useful validation for the change. Do not edit files unless the task explicitly requires test creation.",
            "security_auditor" => "Act as the Canopus security auditor. Identify security risks and recommendations. Do not edit files.",
            "researcher" => "Act as the Canopus researcher. Gather repository-local findings and recommendations. Do not edit files.",
            "test_writer" => "Act as the Canopus test writer. Add or update focused tests for the requested behavior.",
            _ => "Act as the requested Canopus agent role. Keep output concrete and scoped. Do not edit files unless this role explicitly owns implementation.",
        },
    };

    let mut prompt = format!(
        "{role_instruction}\n\nTask metadata:\n- task_id: {}\n- agenda_id: {}\n- role: {}\n- role_mode: {}\n\nUser request:\n{}\n",
        task.id,
        task.agenda_id,
        task.role.as_str(),
        task.role_mode,
        task.prompt
    );

    if !prior_artifacts.is_empty() {
        prompt.push_str("\nPrior artifacts:\n");
        for artifact in prior_artifacts {
            prompt.push_str(&format!(
                "\n## {}\n{}\n",
                artifact.kind.file_name(),
                artifact.content
            ));
        }
    }

    prompt.push_str(
        "\nCanopus completion contract:\n- Stay inside this repository.\n- Preserve existing project boundaries and safety gates.\n- Report what changed, what was verified, and any remaining risk.\n",
    );
    prompt
}

fn sandbox_for_role<'a>(role: &AgentRole, write_sandbox: &'a str) -> &'a str {
    match role {
        AgentRole::Coder => write_sandbox,
        AgentRole::Custom(name) if name == "test_writer" => write_sandbox,
        _ => "read-only",
    }
}

fn artifact_kind_for_role(role: &AgentRole) -> ArtifactKind {
    match role {
        AgentRole::Planner => ArtifactKind::Plan,
        AgentRole::Reviewer => ArtifactKind::Review,
        _ => ArtifactKind::RuntimeLog,
    }
}

fn artifact_content_for_role(_role: &AgentRole, _runtime_log: &str, final_message: &str) -> String {
    final_message.to_string()
}

fn summary_for_role(role: &AgentRole) -> String {
    match role {
        AgentRole::Planner => "codex planner completed".to_string(),
        AgentRole::Coder => "codex coder completed".to_string(),
        AgentRole::Reviewer => "codex reviewer completed".to_string(),
        AgentRole::Custom(name) => format!("codex {name} completed"),
    }
}

fn runtime_log(
    command: &[String],
    status: i32,
    stdout: &str,
    stderr: &str,
    final_message: &str,
) -> String {
    format!(
        "# Codex runtime\n\ncommand: `{}`\nstatus: {}\n\n## final message\n```text\n{}```\n\n## stdout\n```text\n{}```\n\n## stderr\n```text\n{}```\n",
        command.join(" "),
        status,
        final_message,
        stdout,
        stderr
    )
}

fn last_message_path(task: &AgentTask) -> CanopusResult<PathBuf> {
    let mut safe_id = String::new();
    for ch in task.id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            safe_id.push(ch);
        } else {
            safe_id.push('-');
        }
    }
    if safe_id.is_empty() {
        return Err(CanopusError::InvalidInput(
            "task id must not be empty".to_string(),
        ));
    }
    Ok(std::env::temp_dir().join(format!(
        "canopus-codex-{}-{}-last-message.md",
        std::process::id(),
        safe_id
    )))
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
