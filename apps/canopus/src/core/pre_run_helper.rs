use crate::core::{AgentRole, CanopusError, CanopusResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreRunHelperMode {
    Off,
    RepoExplore,
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreRunHelperFailurePolicy {
    Advisory,
    FailFast,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreRunHelperConfig {
    pub mode: PreRunHelperMode,
    pub max_output_bytes: usize,
    pub failure_policy: PreRunHelperFailurePolicy,
}

impl Default for PreRunHelperConfig {
    fn default() -> Self {
        Self {
            mode: PreRunHelperMode::Off,
            max_output_bytes: 6_000,
            failure_policy: PreRunHelperFailurePolicy::Advisory,
        }
    }
}

impl PreRunHelperConfig {
    pub fn from_env() -> CanopusResult<Self> {
        let mut config = Self::default();
        if let Ok(value) = std::env::var("CANOPUS_PRE_RUN_HELPERS") {
            config.mode = parse_mode(&value)?;
        }
        if let Ok(value) = std::env::var("CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES") {
            config.max_output_bytes = value.parse::<usize>().map_err(|_| {
                CanopusError::InvalidInput(format!(
                    "CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES must be a positive integer, got `{value}`"
                ))
            })?;
            if config.max_output_bytes == 0 {
                return Err(CanopusError::InvalidInput(
                    "CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES must be greater than 0".to_string(),
                ));
            }
        }
        if let Ok(value) = std::env::var("CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY") {
            config.failure_policy = parse_failure_policy(&value)?;
        }
        Ok(config)
    }

    pub fn enabled(&self) -> bool {
        self.mode != PreRunHelperMode::Off
    }
}

fn parse_mode(value: &str) -> CanopusResult<PreRunHelperMode> {
    match value.trim() {
        "" | "off" | "0" | "false" | "no" => Ok(PreRunHelperMode::Off),
        "repo-explore" => Ok(PreRunHelperMode::RepoExplore),
        "mock" => Ok(PreRunHelperMode::Mock),
        other => Err(CanopusError::InvalidInput(format!(
            "unsupported CANOPUS_PRE_RUN_HELPERS `{other}` (expected off, repo-explore, or mock)"
        ))),
    }
}

fn parse_failure_policy(value: &str) -> CanopusResult<PreRunHelperFailurePolicy> {
    match value.trim() {
        "" | "advisory" => Ok(PreRunHelperFailurePolicy::Advisory),
        "fail-fast" => Ok(PreRunHelperFailurePolicy::FailFast),
        other => Err(CanopusError::InvalidInput(format!(
            "unsupported CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY `{other}` (expected advisory or fail-fast)"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperRequest {
    pub agenda_id: String,
    pub role_task_id: String,
    pub role: AgentRole,
    pub stage_name: String,
    pub user_request_summary: String,
    pub prior_artifact_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperSelection {
    pub name: String,
    pub mode: PreRunHelperMode,
    pub reason: String,
    pub attach_as_context: bool,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperOutput {
    pub summary: String,
    pub content: String,
    pub truncated: bool,
    pub read_only_check: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperProvenance {
    pub name: String,
    pub role: String,
    pub stage_name: String,
    pub reason: String,
    pub backend_identity: String,
    pub status: String,
    pub summary: String,
    pub artifact_path: String,
    pub attached_to: String,
    pub read_only_check: String,
}

impl HelperProvenance {
    pub fn to_markdown(&self, output: Option<&HelperOutput>) -> String {
        let mut markdown = format!(
            "# Pre-run Helper Provenance\n\n\
helper: {}\n\
role: {}\n\
stage: {}\n\
reason: {}\n\
backend_identity: {}\n\
status: {}\n\
summary: {}\n\
artifact_path: {}\n\
attached_to: {}\n\
read_only_check: {}\n",
            self.name,
            self.role,
            self.stage_name,
            self.reason,
            self.backend_identity,
            self.status,
            self.summary,
            self.artifact_path,
            self.attached_to,
            self.read_only_check
        );
        if let Some(output) = output {
            markdown.push_str(&format!(
                "\n## Helper Output\n\ntruncated: {}\n\n```text\n{}\n```\n",
                output.truncated, output.content
            ));
        }
        markdown
    }
}

pub fn select_pre_run_helpers(
    config: &PreRunHelperConfig,
    role: &AgentRole,
    stage_name: &str,
) -> Vec<HelperSelection> {
    if !config.enabled() || !eligible_role(role) {
        return Vec::new();
    }

    let name = match config.mode {
        PreRunHelperMode::Off => return Vec::new(),
        PreRunHelperMode::RepoExplore => "repo-explore",
        PreRunHelperMode::Mock => "mock-context",
    };

    vec![HelperSelection {
        name: name.to_string(),
        mode: config.mode,
        reason: format!("Canopus-selected read-only context helper before `{stage_name}` stage"),
        attach_as_context: true,
        required: false,
    }]
}

fn eligible_role(role: &AgentRole) -> bool {
    matches!(
        role,
        AgentRole::Planner | AgentRole::Coder | AgentRole::Reviewer
    )
}

pub fn helper_artifact_task_id(role_task_id: &str, helper_name: &str, ordinal: usize) -> String {
    format!(
        "{}-helper-{}-{}",
        sanitize_path_component(role_task_id),
        sanitize_path_component(helper_name),
        ordinal
    )
}

fn sanitize_path_component(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if !last_dash {
            last_dash = true;
            Some('-')
        } else {
            None
        };
        if let Some(ch) = next {
            out.push(ch);
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "helper".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_disables_helpers() {
        let config = PreRunHelperConfig::default();
        assert_eq!(config.mode, PreRunHelperMode::Off);
        assert!(!config.enabled());
        assert!(select_pre_run_helpers(&config, &AgentRole::Planner, "plan").is_empty());
    }

    #[test]
    fn selector_chooses_enabled_eligible_roles_only() {
        let config = PreRunHelperConfig {
            mode: PreRunHelperMode::Mock,
            ..PreRunHelperConfig::default()
        };
        assert_eq!(
            select_pre_run_helpers(&config, &AgentRole::Planner, "plan")[0].name,
            "mock-context"
        );
        assert!(select_pre_run_helpers(
            &config,
            &AgentRole::Custom("analyst".to_string()),
            "analyst"
        )
        .is_empty());
    }

    #[test]
    fn helper_artifact_ids_are_path_safe_and_unique() {
        assert_eq!(
            helper_artifact_task_id("TASK/Plan", "$repo explore", 0),
            "task-plan-helper-repo-explore-0"
        );
        assert_ne!(
            helper_artifact_task_id("TASK", "repo", 0),
            helper_artifact_task_id("TASK", "repo", 1)
        );
    }

    #[test]
    fn provenance_markdown_includes_mandatory_fields() {
        let provenance = HelperProvenance {
            name: "repo-explore".to_string(),
            role: "reviewer".to_string(),
            stage_name: "review".to_string(),
            reason: "reason".to_string(),
            backend_identity: "omx explore".to_string(),
            status: "ok".to_string(),
            summary: "summary".to_string(),
            artifact_path: "artifact".to_string(),
            attached_to: "prior_artifacts before TASK".to_string(),
            read_only_check: "passed".to_string(),
        };
        let markdown = provenance.to_markdown(None);
        for needle in [
            "helper: repo-explore",
            "role: reviewer",
            "stage: review",
            "backend_identity: omx explore",
            "attached_to: prior_artifacts before TASK",
            "read_only_check: passed",
        ] {
            assert!(markdown.contains(needle), "missing {needle} in {markdown}");
        }
    }
}
