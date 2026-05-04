use super::{print_json, require_existing_repo, require_gate, require_token};
use crate::adapters::github::ProjectOwnerKind;
use crate::cli::args::{env_flag, env_u64, ProjectRegisterArgs};
use crate::core::{CanopusError, CanopusResult};
use serde::Serialize;

pub(crate) fn project_register(args: &[String]) -> CanopusResult<()> {
    let parsed = ProjectRegisterArgs::parse(args)?;
    require_existing_repo(&parsed.repo)?;
    require_gate("CANOPUS_ENABLE_GITHUB")?;
    require_gate("CANOPUS_ENABLE_LIVE_MUTATIONS")?;
    require_gate("CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION")?;
    require_gate("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION")?;
    if parsed.create_github_repo {
        require_gate("CANOPUS_ALLOW_GITHUB_REPO_CREATE")?;
    }
    if !env_flag("CANOPUS_MOCK_GITHUB") {
        require_token()?;
        return Err(CanopusError::Tool(
            "live GitHub project registration is not implemented in this offline-safe build; use CANOPUS_MOCK_GITHUB=1 in tests".to_string(),
        ));
    }

    let owner = parsed.github_owner.clone();
    let repo = parsed.github_repo.clone();
    let project_number = env_u64("CANOPUS_MOCK_GITHUB_PROJECT_NUMBER")?.unwrap_or(1);
    let home_issue_number = env_u64("CANOPUS_MOCK_GITHUB_HOME_ISSUE_NUMBER")?.unwrap_or(1);
    let registry = ProjectRegistrationOutput {
        name: parsed
            .repo
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string(),
        repo_path: parsed.repo.display().to_string(),
        github_owner: owner.clone(),
        github_repo: repo.clone(),
        github_repo_url: format!("https://github.com/{owner}/{repo}"),
        github_repo_node_id: format!("R_mock_{owner}_{repo}"),
        github_project_id: format!("PVT_mock_{owner}_{repo}"),
        github_project_url: format!(
            "https://github.com/{}/{}/projects/{project_number}",
            parsed.project_owner_kind.github_url_segment(),
            parsed.project_owner
        ),
        github_project_owner_kind: parsed.project_owner_kind.as_str().to_string(),
        github_project_owner: parsed.project_owner.clone(),
        github_project_number: project_number,
        github_home_issue_number: home_issue_number,
        github_home_issue_node_id: format!("I_mock_home_{home_issue_number}"),
        github_home_issue_url: format!(
            "https://github.com/{owner}/{repo}/issues/{home_issue_number}"
        ),
        github_project_item_id: format!("PVTI_mock_home_{home_issue_number}"),
        github_project_status_field_name: "Status".to_string(),
        github_project_status_option_name: "Registered".to_string(),
        github_project_status: "Registered".to_string(),
        audit: vec![
            "validate_repository".to_string(),
            if parsed.create_github_repo {
                "create_repository"
            } else {
                "skip_repository_create"
            }
            .to_string(),
            "create_project_v2".to_string(),
            "create_home_issue".to_string(),
            "add_home_issue_to_project".to_string(),
            "update_project_status".to_string(),
        ],
    };
    print_json(&registry)
}

#[derive(Debug, Clone, Serialize)]
struct ProjectRegistrationOutput {
    name: String,
    repo_path: String,
    github_owner: String,
    github_repo: String,
    github_repo_url: String,
    github_repo_node_id: String,
    github_project_id: String,
    github_project_url: String,
    github_project_owner_kind: String,
    github_project_owner: String,
    github_project_number: u64,
    github_home_issue_number: u64,
    github_home_issue_node_id: String,
    github_home_issue_url: String,
    github_project_item_id: String,
    github_project_status_field_name: String,
    github_project_status_option_name: String,
    github_project_status: String,
    audit: Vec<String>,
}

impl ProjectOwnerKind {
    fn github_url_segment(self) -> &'static str {
        match self {
            ProjectOwnerKind::Org => "orgs",
            ProjectOwnerKind::User => "users",
        }
    }
}
