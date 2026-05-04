use super::{print_json, require_existing_repo, require_gate, require_token};
use crate::cli::args::{env_flag, env_u64, WorkIntakeArgs};
use crate::core::{CanopusError, CanopusResult};
use serde::Serialize;

pub(crate) fn work_intake(args: &[String]) -> CanopusResult<()> {
    let parsed = WorkIntakeArgs::parse(args)?;
    require_existing_repo(&parsed.repo)?;
    require_gate("CANOPUS_ENABLE_GITHUB")?;
    require_gate("CANOPUS_ENABLE_LIVE_MUTATIONS")?;
    require_gate("CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION")?;
    require_gate("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION")?;
    if !env_flag("CANOPUS_MOCK_GITHUB") {
        require_token()?;
        return Err(CanopusError::Tool(
            "live GitHub work intake is not implemented in this offline-safe build; use CANOPUS_MOCK_GITHUB=1 in tests".to_string(),
        ));
    }

    let registration = parsed.registration_json()?;
    let owner = registration
        .get("github_owner")
        .and_then(|v| v.as_str())
        .unwrap_or("owner");
    let repo = registration
        .get("github_repo")
        .and_then(|v| v.as_str())
        .unwrap_or("repo");
    let project_id = registration
        .get("github_project_id")
        .and_then(|v| v.as_str())
        .unwrap_or("PVT_mock_project");
    let project_url = registration
        .get("github_project_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let home_issue_number = registration
        .get("github_home_issue_number")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let issue_number = env_u64("CANOPUS_MOCK_GITHUB_WORK_ISSUE_NUMBER")?.unwrap_or(2);
    let output = WorkIntakeOutput {
        task_id: parsed.task_id,
        agenda_id: parsed.agenda_id,
        github_owner: owner.to_string(),
        github_repo: repo.to_string(),
        github_issue_number: issue_number,
        github_issue_node_id: format!("I_mock_work_{issue_number}"),
        github_issue_url: format!("https://github.com/{owner}/{repo}/issues/{issue_number}"),
        github_home_issue_number: home_issue_number,
        github_project_id: project_id.to_string(),
        github_project_url: project_url.to_string(),
        github_project_item_id: format!("PVTI_mock_work_{issue_number}"),
        github_project_status: "Issue Created".to_string(),
        request: parsed.request,
        discord_message_url: parsed.discord_message_url,
        audit: vec![
            "create_work_issue".to_string(),
            "link_home_issue".to_string(),
            "add_work_issue_to_project".to_string(),
            "update_project_status".to_string(),
        ],
    };
    print_json(&output)
}

#[derive(Debug, Clone, Serialize)]
struct WorkIntakeOutput {
    task_id: String,
    agenda_id: String,
    github_owner: String,
    github_repo: String,
    github_issue_number: u64,
    github_issue_node_id: String,
    github_issue_url: String,
    github_home_issue_number: u64,
    github_project_id: String,
    github_project_url: String,
    github_project_item_id: String,
    github_project_status: String,
    request: String,
    discord_message_url: Option<String>,
    audit: Vec<String>,
}
