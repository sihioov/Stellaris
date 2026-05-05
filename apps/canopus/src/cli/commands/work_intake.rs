use super::{print_json, require_existing_repo, require_gate, require_token};
use crate::adapters::github::{
    GitHubClient, GitHubIssueCreated, GitHubProjectGates, GitHubProjectSyncConfig, ProjectOwnerKind,
};
use crate::cli::args::{env_flag, env_non_empty, env_u64, ProjectSyncPolicy, WorkIntakeArgs};
use crate::core::{CanopusError, CanopusResult, GitHubProjectMode};
use serde::Serialize;
use serde_json::Value;

pub(crate) fn work_intake(args: &[String]) -> CanopusResult<()> {
    let parsed = WorkIntakeArgs::parse(args)?;
    require_existing_repo(&parsed.repo)?;
    require_gate("CANOPUS_ENABLE_GITHUB")?;
    require_gate("CANOPUS_ENABLE_LIVE_MUTATIONS")?;
    require_gate("CANOPUS_ALLOW_GITHUB_MUTATION")?;

    let registration = parsed.registration_json()?;
    let owner = required_non_empty(&registration, "github_owner")?;
    let repo = required_non_empty(&registration, "github_repo")?;
    let project_inputs = ProjectSyncInputs::from_registration(&registration, &owner, &repo)?;
    project_inputs.preflight_required(parsed.project_sync)?;

    let issue = create_work_issue(&parsed, &owner, &repo)?;
    let (project_sync, project_failure) = project_sync_status(&parsed, &project_inputs, &issue);

    let output = WorkIntakeOutput {
        ok: true,
        task_id: parsed.task_id.clone(),
        agenda_id: parsed.agenda_id.clone(),
        github_owner: owner.clone(),
        github_repo: repo.clone(),
        github_issue_number: issue.number,
        github_issue_node_id: non_empty_option(issue.node_id.clone()),
        github_issue_url: issue.url.clone(),
        github_home_issue_number: project_inputs.home_issue_number,
        github_project_id: project_sync.github_project_id.clone(),
        github_project_url: project_sync.github_project_url.clone(),
        github_project_item_id: project_sync.github_project_item_id.clone(),
        github_project_status: project_sync.github_project_status.clone(),
        request: parsed.request.clone(),
        discord_message_url: parsed.discord_message_url.clone(),
        project_sync: project_sync.clone(),
        audit: audit_steps(&parsed.project_sync, &project_sync),
    };

    if let Some(error) = project_failure {
        let partial = PartialFailureOutput::from_success(output, error.clone());
        print_json(&partial)?;
        return Err(CanopusError::Tool(format!(
            "GitHub Project sync failed after Issue creation: {error}"
        )));
    }

    print_json(&output)
}

#[derive(Debug, Clone, Serialize)]
struct WorkIntakeOutput {
    ok: bool,
    task_id: String,
    agenda_id: String,
    github_owner: String,
    github_repo: String,
    github_issue_number: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_issue_node_id: Option<String>,
    github_issue_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_home_issue_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_status: Option<String>,
    request: String,
    discord_message_url: Option<String>,
    project_sync: ProjectSyncOutput,
    audit: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PartialFailureOutput {
    ok: bool,
    error: String,
    task_id: String,
    agenda_id: String,
    github_owner: String,
    github_repo: String,
    github_issue_number: u64,
    github_issue_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_issue_node_id: Option<String>,
    project_sync: ProjectSyncOutput,
    audit: Vec<String>,
}

impl PartialFailureOutput {
    fn from_success(output: WorkIntakeOutput, error: String) -> Self {
        Self {
            ok: false,
            error,
            task_id: output.task_id,
            agenda_id: output.agenda_id,
            github_owner: output.github_owner,
            github_repo: output.github_repo,
            github_issue_number: output.github_issue_number,
            github_issue_url: output.github_issue_url,
            github_issue_node_id: output.github_issue_node_id,
            project_sync: output.project_sync,
            audit: output.audit,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProjectSyncOutput {
    policy: String,
    requested: bool,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    github_project_status: Option<String>,
    executed_operations: Vec<String>,
}

#[derive(Debug, Clone)]
struct ProjectSyncInputs {
    project_id: Option<String>,
    project_url: Option<String>,
    project_owner_kind: Option<ProjectOwnerKind>,
    project_owner: Option<String>,
    project_number: Option<u64>,
    repo_owner: String,
    repo_name: String,
    home_issue_number: Option<u64>,
    status_field_id: Option<String>,
    status_field_name: Option<String>,
    status_option_id: Option<String>,
    status_option_name: Option<String>,
    status: Option<String>,
}

impl ProjectSyncInputs {
    fn from_registration(
        registration: &serde_json::Map<String, Value>,
        repo_owner: &str,
        repo_name: &str,
    ) -> CanopusResult<Self> {
        let project_owner_kind = optional_string(registration, "github_project_owner_kind")
            .as_deref()
            .map(ProjectOwnerKind::parse)
            .transpose()?;
        Ok(Self {
            project_id: optional_string(registration, "github_project_id"),
            project_url: optional_string(registration, "github_project_url"),
            project_owner_kind,
            project_owner: optional_string(registration, "github_project_owner"),
            project_number: optional_u64(registration, "github_project_number"),
            repo_owner: repo_owner.to_string(),
            repo_name: repo_name.to_string(),
            home_issue_number: optional_u64(registration, "github_home_issue_number"),
            status_field_id: optional_string(registration, "github_project_status_field_id"),
            status_field_name: optional_string(registration, "github_project_status_field_name"),
            status_option_id: optional_string(registration, "github_project_status_option_id"),
            status_option_name: optional_string(registration, "github_project_status_option_name"),
            status: optional_string(registration, "github_project_status"),
        })
    }

    fn preflight_required(&self, policy: ProjectSyncPolicy) -> CanopusResult<()> {
        if !matches!(policy, ProjectSyncPolicy::Required) {
            return Ok(());
        }
        let missing = self.missing_project_sync_requirements(true);
        if missing.is_empty() {
            return Ok(());
        }
        Err(CanopusError::InvalidInput(format!(
            "work-intake --project-sync required cannot create an Issue until GitHub Project sync preflight passes: {}",
            missing.join(", ")
        )))
    }

    fn project_identity_present(&self) -> bool {
        self.project_id.is_some()
            || self.project_url.is_some()
            || (self.project_owner_kind.is_some()
                && self.project_owner.is_some()
                && self.project_number.is_some())
    }

    fn status_target_present(&self) -> bool {
        self.status.is_some()
            || self.status_option_id.is_some()
            || self.status_option_name.is_some()
    }

    fn missing_project_sync_requirements(&self, include_gates: bool) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.project_identity_present() {
            missing.push("project identity (github_project_id, github_project_url, or owner_kind+owner+number)".to_string());
        }
        if !self.status_target_present() {
            missing.push("status target (github_project_status, github_project_status_option_id, or github_project_status_option_name)".to_string());
        }
        if include_gates {
            for gate in [
                "CANOPUS_ENABLE_GITHUB",
                "CANOPUS_ENABLE_LIVE_MUTATIONS",
                "CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION",
            ] {
                if !env_flag(gate) {
                    missing.push(format!("{gate}=1"));
                }
            }
        }
        missing
    }

    fn to_config(&self, issue_number: u64) -> GitHubProjectSyncConfig {
        GitHubProjectSyncConfig {
            mode: GitHubProjectMode::MutateLive,
            project_id: self.project_id.clone(),
            project_url: self.project_url.clone(),
            project_owner_kind: self.project_owner_kind,
            project_owner: self.project_owner.clone(),
            project_number: self.project_number,
            repo_owner: Some(self.repo_owner.clone()),
            repo_name: Some(self.repo_name.clone()),
            issue_number: Some(issue_number),
            project_item_id: None,
            status_field_id: self.status_field_id.clone(),
            status_field_name: self.status_field_name.clone(),
            status_option_id: self.status_option_id.clone(),
            status_option_name: self.status_option_name.clone(),
            status: self.status.clone(),
        }
    }
}

fn create_work_issue(
    parsed: &WorkIntakeArgs,
    owner: &str,
    repo: &str,
) -> CanopusResult<GitHubIssueCreated> {
    if env_flag("CANOPUS_MOCK_GITHUB") {
        let number = env_u64("CANOPUS_MOCK_GITHUB_WORK_ISSUE_NUMBER")?.unwrap_or(2);
        return Ok(GitHubIssueCreated {
            number,
            url: format!("https://github.com/{owner}/{repo}/issues/{number}"),
            node_id: format!("I_mock_work_{number}"),
        });
    }

    require_token()?;
    let token = env_non_empty("GITHUB_TOKEN").ok_or_else(|| {
        CanopusError::InvalidInput("GITHUB_TOKEN required for live GitHub mutation".to_string())
    })?;
    let client = GitHubClient::new(&token, owner, repo);
    client.create_issue(&work_issue_title(parsed), &work_issue_body(parsed))
}

fn project_sync_status(
    parsed: &WorkIntakeArgs,
    inputs: &ProjectSyncInputs,
    issue: &GitHubIssueCreated,
) -> (ProjectSyncOutput, Option<String>) {
    let policy = parsed.project_sync;
    let base = |requested: bool, status: &str, reason: Option<String>| ProjectSyncOutput {
        policy: policy.as_str().to_string(),
        requested,
        status: status.to_string(),
        reason,
        github_project_id: None,
        github_project_url: inputs.project_url.clone(),
        github_project_item_id: None,
        github_project_status: None,
        executed_operations: Vec::new(),
    };

    if matches!(policy, ProjectSyncPolicy::Off) {
        return (
            base(false, "skipped", Some("project sync disabled".to_string())),
            None,
        );
    }

    let missing = inputs.missing_project_sync_requirements(true);
    if matches!(policy, ProjectSyncPolicy::BestEffort) && !missing.is_empty() {
        return (
            base(
                false,
                "skipped",
                Some(format!(
                    "best-effort preflight incomplete: {}",
                    missing.join(", ")
                )),
            ),
            None,
        );
    }

    if env_flag("CANOPUS_MOCK_GITHUB_PROJECT_SYNC_FAIL") {
        return (
            base(
                true,
                "failed",
                Some("mock project sync failure".to_string()),
            ),
            Some("mock project sync failure".to_string()),
        );
    }

    if env_flag("CANOPUS_MOCK_GITHUB") {
        let item_id = format!("PVTI_mock_work_{}", issue.number);
        let mut output = base(true, "succeeded", None);
        output.github_project_id = inputs
            .project_id
            .clone()
            .or_else(|| Some("PVT_mock_project".to_string()));
        output.github_project_item_id = Some(item_id);
        output.github_project_status = inputs
            .status
            .clone()
            .or_else(|| inputs.status_option_name.clone())
            .or_else(|| Some("Issue Created".to_string()));
        output.executed_operations = vec![
            "add_work_issue_to_project".to_string(),
            "update_project_status".to_string(),
        ];
        return (output, None);
    }

    let token = match env_non_empty("GITHUB_TOKEN") {
        Some(token) => token,
        None => {
            let err = "GITHUB_TOKEN required for live GitHub Project sync".to_string();
            return (base(true, "failed", Some(err.clone())), Some(err));
        }
    };
    let client = GitHubClient::new(&token, &inputs.repo_owner, &inputs.repo_name);
    let gates = GitHubProjectGates {
        enable_github: env_flag("CANOPUS_ENABLE_GITHUB"),
        enable_live_mutations: env_flag("CANOPUS_ENABLE_LIVE_MUTATIONS"),
        allow_project_mutation: env_flag("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION"),
    };
    match client.sync_project_v2(&inputs.to_config(issue.number), &gates) {
        Ok(report) => {
            let mut output = base(true, "succeeded", None);
            output.github_project_id = report.project_id;
            output.github_project_item_id = report.item_id;
            output.github_project_status = inputs
                .status
                .clone()
                .or_else(|| inputs.status_option_name.clone());
            output.executed_operations = report
                .executed_operations
                .into_iter()
                .map(ToString::to_string)
                .collect();
            (output, None)
        }
        Err(err) => {
            let message = err.to_string();
            (base(true, "failed", Some(message.clone())), Some(message))
        }
    }
}

fn audit_steps(policy: &ProjectSyncPolicy, project_sync: &ProjectSyncOutput) -> Vec<String> {
    let mut audit = vec!["create_work_issue".to_string()];
    if project_sync.requested {
        audit.extend(project_sync.executed_operations.iter().cloned());
    } else if matches!(
        policy,
        ProjectSyncPolicy::Off | ProjectSyncPolicy::BestEffort
    ) {
        audit.push("skip_project_sync".to_string());
    }
    audit
}

fn work_issue_title(parsed: &WorkIntakeArgs) -> String {
    format!("[Canopus Work] {}", truncate_for_title(&parsed.request, 80))
}

fn work_issue_body(parsed: &WorkIntakeArgs) -> String {
    let mut body = format!(
        "{}\n\nTask: `{}`\nAgenda: `{}`",
        parsed.request, parsed.task_id, parsed.agenda_id
    );
    if let Some(url) = parsed.discord_message_url.as_deref() {
        body.push_str(&format!("\nDiscord: {url}"));
    }
    body
}

fn truncate_for_title(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        compact
    } else {
        let mut out = compact
            .chars()
            .take(limit.saturating_sub(1))
            .collect::<String>();
        out.push('…');
        out
    }
}

fn required_non_empty(
    registration: &serde_json::Map<String, Value>,
    key: &str,
) -> CanopusResult<String> {
    optional_string(registration, key).ok_or_else(|| {
        CanopusError::InvalidInput(format!(
            "work-intake registration requires non-empty {key}; fake owner/repo defaults are not allowed"
        ))
    })
}

fn optional_string(registration: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    registration.get(key).and_then(|value| match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn optional_u64(registration: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    registration.get(key).and_then(|value| match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    })
}

fn non_empty_option(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}
