use crate::core::{CanopusError, CanopusResult, GitHubProjectMode};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct GitHubClient {
    token: String,
    owner: String,
    repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubIssueCreated {
    pub number: u64,
    pub url: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphQlOperationKind {
    Query,
    Mutation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphQlOperation {
    pub operation_name: &'static str,
    pub kind: GraphQlOperationKind,
    pub query: &'static str,
    pub variables: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectOwnerKind {
    User,
    Org,
}

impl ProjectOwnerKind {
    pub fn parse(value: &str) -> CanopusResult<Self> {
        match value {
            "user" => Ok(Self::User),
            "org" => Ok(Self::Org),
            _ => Err(CanopusError::InvalidInput(format!(
                "github project owner kind must be `user` or `org`, got `{value}`"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Org => "org",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectUrlParts {
    pub owner_kind: ProjectOwnerKind,
    pub owner: String,
    pub number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubProjectGates {
    pub enable_github: bool,
    pub enable_live_mutations: bool,
    pub allow_project_mutation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubProjectSyncConfig {
    pub mode: GitHubProjectMode,
    pub project_id: Option<String>,
    pub project_url: Option<String>,
    pub project_owner_kind: Option<ProjectOwnerKind>,
    pub project_owner: Option<String>,
    pub project_number: Option<u64>,
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub issue_number: Option<u64>,
    pub project_item_id: Option<String>,
    pub status_field_id: Option<String>,
    pub status_field_name: Option<String>,
    pub status_option_id: Option<String>,
    pub status_option_name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubProjectSyncPlan {
    pub mode: GitHubProjectMode,
    pub project_id_source: String,
    pub item_id_source: String,
    pub status_field_source: String,
    pub status_option_source: String,
    pub operations: Vec<GraphQlOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubProjectSyncReport {
    pub mode: GitHubProjectMode,
    pub plan: GitHubProjectSyncPlan,
    pub project_id: Option<String>,
    pub item_id: Option<String>,
    pub executed_operations: Vec<&'static str>,
}

#[derive(Debug, Deserialize)]
pub struct IssueComment {
    pub id: u64,
    pub body: String,
    pub user: CommentUser,
}

#[derive(Debug, Deserialize)]
pub struct CommentUser {
    pub login: String,
}

impl GitHubClient {
    pub fn new(token: &str, owner: &str, repo: &str) -> Self {
        Self {
            token: token.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        }
    }

    /// 환경변수에서 GitHubClient 생성. GITHUB_TOKEN, GITHUB_OWNER, GITHUB_REPO 필요.
    /// 환경변수 없으면 None 반환.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("GITHUB_TOKEN").ok()?;
        let owner = std::env::var("GITHUB_OWNER").ok()?;
        let repo = std::env::var("GITHUB_REPO").ok()?;
        Some(Self::new(&token, &owner, &repo))
    }

    fn base_url(&self) -> String {
        format!("https://api.github.com/repos/{}/{}", self.owner, self.repo)
    }

    fn graphql_url(&self) -> &'static str {
        "https://api.github.com/graphql"
    }

    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new().user_agent("canopus/0.1").build()
    }

    pub fn graphql(&self, operation: &GraphQlOperation) -> CanopusResult<Value> {
        let payload = json!({
            "query": operation.query,
            "variables": operation.variables,
            "operationName": operation.operation_name,
        });
        self.agent()
            .post(self.graphql_url())
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .send_json(payload)
            .map_err(|e| CanopusError::Tool(e.to_string()))?
            .into_json()
            .map_err(|e| CanopusError::Tool(e.to_string()))
    }

    pub fn sync_project_v2(
        &self,
        config: &GitHubProjectSyncConfig,
        gates: &GitHubProjectGates,
    ) -> CanopusResult<GitHubProjectSyncReport> {
        let plan = build_project_sync_plan(config, gates)?;
        if matches!(config.mode, GitHubProjectMode::DryRunOffline) {
            return Ok(GitHubProjectSyncReport {
                mode: config.mode,
                plan,
                project_id: config.project_id.clone(),
                item_id: config.project_item_id.clone(),
                executed_operations: Vec::new(),
            });
        }

        let mut executed_operations = Vec::new();
        if matches!(config.mode, GitHubProjectMode::ValidateReadOnly) {
            let project_id =
                self.validate_project_v2_read_only(config, &mut executed_operations)?;
            return Ok(GitHubProjectSyncReport {
                mode: config.mode,
                plan,
                project_id,
                item_id: config.project_item_id.clone(),
                executed_operations,
            });
        }

        let (project_id, project_id_source) = if let Some(project_id) = config
            .project_id
            .as_deref()
            .filter(|project_id| !project_id.trim().is_empty())
        {
            (project_id.to_string(), "provided")
        } else {
            let (owner_kind, owner, number) = project_lookup_parts(config)?;
            let operation = build_project_lookup_query(owner_kind, &owner, number);
            let response = self.graphql(&operation)?;
            executed_operations.push(operation.operation_name);
            (
                parse_project_lookup_id(&response, owner_kind)?,
                owner_kind.as_str(),
            )
        };
        log::info!("[github-project] project id source: {project_id_source}");

        let item_id = if let Some(item_id) = config
            .project_item_id
            .as_deref()
            .filter(|item_id| !item_id.trim().is_empty())
        {
            item_id.to_string()
        } else {
            let (owner, repo, number) = project_content_lookup_inputs(config)?;
            let content_operation = build_content_lookup_query(owner, repo, number);
            let content_response = self.graphql(&content_operation)?;
            executed_operations.push(content_operation.operation_name);
            let content_id = parse_content_lookup_id(&content_response)?;

            let add_operation = build_add_project_item_mutation(&project_id, &content_id);
            let add_response = self.graphql(&add_operation)?;
            executed_operations.push(add_operation.operation_name);
            parse_added_project_item_id(&add_response)?
        };

        let (field_id, option_id) = match (
            config
                .status_field_id
                .as_deref()
                .filter(|field_id| !field_id.trim().is_empty()),
            config
                .status_option_id
                .as_deref()
                .filter(|option_id| !option_id.trim().is_empty()),
        ) {
            (Some(field_id), Some(option_id)) => (field_id.to_string(), option_id.to_string()),
            (field_id, option_id) => {
                let operation = build_project_fields_query(&project_id);
                let response = self.graphql(&operation)?;
                executed_operations.push(operation.operation_name);
                let requested_field = requested_status_field_name(config);
                let requested_option = requested_status_option_name(config, "status update")?;
                let (resolved_field_id, resolved_option_id) = resolve_single_select_field_option(
                    &response,
                    requested_field,
                    requested_option,
                )?;
                (
                    field_id.unwrap_or(&resolved_field_id).to_string(),
                    option_id.unwrap_or(&resolved_option_id).to_string(),
                )
            }
        };

        let update_operation =
            build_update_project_status_mutation(&project_id, &item_id, &field_id, &option_id);
        self.graphql(&update_operation)?;
        executed_operations.push(update_operation.operation_name);

        Ok(GitHubProjectSyncReport {
            mode: config.mode,
            plan,
            project_id: Some(project_id),
            item_id: Some(item_id),
            executed_operations,
        })
    }

    fn validate_project_v2_read_only(
        &self,
        config: &GitHubProjectSyncConfig,
        executed_operations: &mut Vec<&'static str>,
    ) -> CanopusResult<Option<String>> {
        let project_id = if let Some(project_id) = config
            .project_id
            .as_deref()
            .filter(|project_id| !project_id.trim().is_empty())
        {
            project_id.to_string()
        } else {
            let (owner_kind, owner, number) = project_lookup_parts(config)?;
            let operation = build_project_lookup_query(owner_kind, &owner, number);
            let response = self.graphql(&operation)?;
            executed_operations.push(operation.operation_name);
            parse_project_lookup_id(&response, owner_kind)?
        };

        if config.project_item_id.is_none() {
            let (owner, repo, number) = project_content_lookup_inputs(config)?;
            let operation = build_content_lookup_query(owner, repo, number);
            self.graphql(&operation)?;
            executed_operations.push(operation.operation_name);
        }

        let needs_fields_query = config.status_field_id.is_none()
            || (config.status_option_id.is_none()
                && (config.status_option_name.is_some() || config.status.is_some()));
        if needs_fields_query {
            let operation = build_project_fields_query(&project_id);
            let response = self.graphql(&operation)?;
            executed_operations.push(operation.operation_name);
            if config.status_field_id.is_none() || config.status_option_id.is_none() {
                let requested_field = requested_status_field_name(config);
                let requested_option =
                    requested_status_option_name(config, "read-only validation")?;
                resolve_single_select_field_option(&response, requested_field, requested_option)?;
            }
        }

        Ok(Some(project_id))
    }

    /// GitHub Issue 생성. 성공 시 issue metadata 반환.
    pub fn create_issue(&self, title: &str, body: &str) -> CanopusResult<GitHubIssueCreated> {
        let url = format!("{}/issues", self.base_url());
        let payload = serde_json::json!({ "title": title, "body": body });
        let resp: serde_json::Value = self
            .agent()
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .send_json(payload)
            .map_err(|e| CanopusError::Tool(e.to_string()))?
            .into_json()
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        let number = resp["number"]
            .as_u64()
            .ok_or_else(|| CanopusError::Tool("missing issue number in response".into()))?;
        let url = resp["html_url"]
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "https://github.com/{}/{}/issues/{number}",
                    self.owner, self.repo
                )
            });
        let node_id = resp["node_id"]
            .as_str()
            .map(ToString::to_string)
            .unwrap_or_default();
        Ok(GitHubIssueCreated {
            number,
            url,
            node_id,
        })
    }

    /// Issue의 모든 comment 조회.
    pub fn get_issue_comments(&self, issue_number: u64) -> CanopusResult<Vec<IssueComment>> {
        let url = format!("{}/issues/{}/comments", self.base_url(), issue_number);
        let resp: Vec<IssueComment> = self
            .agent()
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(|e| CanopusError::Tool(e.to_string()))?
            .into_json()
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        Ok(resp)
    }

    /// Issue close.
    pub fn close_issue(&self, issue_number: u64) -> CanopusResult<()> {
        let url = format!("{}/issues/{}", self.base_url(), issue_number);
        let payload = serde_json::json!({ "state": "closed" });
        self.agent()
            .patch(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .send_json(payload)
            .map_err(|e| CanopusError::Tool(e.to_string()))?;
        Ok(())
    }
}

const PROJECT_LOOKUP_USER_QUERY: &str = r#"query ProjectV2UserLookup($owner: String!, $number: Int!) {
  user(login: $owner) { projectV2(number: $number) { id } }
}"#;

const PROJECT_LOOKUP_ORG_QUERY: &str = r#"query ProjectV2OrgLookup($owner: String!, $number: Int!) {
  organization(login: $owner) { projectV2(number: $number) { id } }
}"#;

const CONTENT_LOOKUP_QUERY: &str = r#"query ProjectV2ContentLookup($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    issue(number: $number) { id }
    pullRequest(number: $number) { id }
  }
}"#;

const FIELDS_QUERY: &str = r#"query ProjectV2Fields($project: ID!) {
  node(id: $project) {
    ... on ProjectV2 {
      fields(first: 20) {
        nodes {
          ... on ProjectV2Field { id name }
          ... on ProjectV2SingleSelectField { id name options { id name } }
        }
      }
    }
  }
}"#;

const ADD_ITEM_MUTATION: &str = r#"mutation AddProjectV2ItemById($project: ID!, $content: ID!) {
  addProjectV2ItemById(input: { projectId: $project, contentId: $content }) { item { id } }
}"#;

const UPDATE_STATUS_MUTATION: &str = r#"mutation UpdateProjectV2ItemFieldValue($project: ID!, $item: ID!, $field: ID!, $option: String!) {
  updateProjectV2ItemFieldValue(input: {
    projectId: $project
    itemId: $item
    fieldId: $field
    value: { singleSelectOptionId: $option }
  }) { projectV2Item { id } }
}"#;

pub fn build_project_lookup_query(
    owner_kind: ProjectOwnerKind,
    owner: &str,
    number: u64,
) -> GraphQlOperation {
    GraphQlOperation {
        operation_name: match owner_kind {
            ProjectOwnerKind::User => "ProjectV2UserLookup",
            ProjectOwnerKind::Org => "ProjectV2OrgLookup",
        },
        kind: GraphQlOperationKind::Query,
        query: match owner_kind {
            ProjectOwnerKind::User => PROJECT_LOOKUP_USER_QUERY,
            ProjectOwnerKind::Org => PROJECT_LOOKUP_ORG_QUERY,
        },
        variables: json!({ "owner": owner, "number": number }),
    }
}

pub fn build_content_lookup_query(owner: &str, repo: &str, number: u64) -> GraphQlOperation {
    GraphQlOperation {
        operation_name: "ProjectV2ContentLookup",
        kind: GraphQlOperationKind::Query,
        query: CONTENT_LOOKUP_QUERY,
        variables: json!({ "owner": owner, "repo": repo, "number": number }),
    }
}

pub fn build_project_fields_query(project_id: &str) -> GraphQlOperation {
    GraphQlOperation {
        operation_name: "ProjectV2Fields",
        kind: GraphQlOperationKind::Query,
        query: FIELDS_QUERY,
        variables: json!({ "project": project_id }),
    }
}

pub fn build_add_project_item_mutation(project_id: &str, content_id: &str) -> GraphQlOperation {
    GraphQlOperation {
        operation_name: "AddProjectV2ItemById",
        kind: GraphQlOperationKind::Mutation,
        query: ADD_ITEM_MUTATION,
        variables: json!({ "project": project_id, "content": content_id }),
    }
}

pub fn build_update_project_status_mutation(
    project_id: &str,
    item_id: &str,
    field_id: &str,
    option_id: &str,
) -> GraphQlOperation {
    GraphQlOperation {
        operation_name: "UpdateProjectV2ItemFieldValue",
        kind: GraphQlOperationKind::Mutation,
        query: UPDATE_STATUS_MUTATION,
        variables: json!({
            "project": project_id,
            "item": item_id,
            "field": field_id,
            "option": option_id,
        }),
    }
}

pub fn parse_project_url(url: &str) -> CanopusResult<ProjectUrlParts> {
    let prefix = "https://github.com/";
    let Some(rest) = url.strip_prefix(prefix) else {
        return Err(CanopusError::InvalidInput(format!(
            "unsupported GitHub Project URL `{url}`; expected https://github.com/users/<owner>/projects/<number> or https://github.com/orgs/<owner>/projects/<number>"
        )));
    };
    let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
    if parts.len() != 4 || parts[2] != "projects" {
        return Err(CanopusError::InvalidInput(format!(
            "unsupported GitHub Project URL `{url}`"
        )));
    }
    let owner_kind = match parts[0] {
        "users" => ProjectOwnerKind::User,
        "orgs" => ProjectOwnerKind::Org,
        _ => {
            return Err(CanopusError::InvalidInput(format!(
                "unsupported GitHub Project owner kind in `{url}`"
            )))
        }
    };
    let number = parts[3].parse::<u64>().map_err(|_| {
        CanopusError::InvalidInput(format!(
            "GitHub Project URL has non-numeric project number: `{url}`"
        ))
    })?;
    Ok(ProjectUrlParts {
        owner_kind,
        owner: parts[1].to_string(),
        number,
    })
}

pub fn build_project_sync_plan(
    config: &GitHubProjectSyncConfig,
    gates: &GitHubProjectGates,
) -> CanopusResult<GitHubProjectSyncPlan> {
    enforce_project_gates(config.mode, gates)?;

    let mut operations = Vec::new();
    let (project_id_source, project_id_for_known_ops) = match config.project_id.as_deref() {
        Some(project_id) if !project_id.trim().is_empty() => {
            ("node_id".to_string(), Some(project_id.to_string()))
        }
        _ => {
            let (owner_kind, owner, number) = project_lookup_parts(config)?;
            operations.push(build_project_lookup_query(owner_kind, &owner, number));
            ("lookup".to_string(), None)
        }
    };

    let item_id_source = if config.project_item_id.is_some() {
        "provided".to_string()
    } else {
        if let (Some(owner), Some(repo), Some(number)) = (
            config.repo_owner.as_deref(),
            config.repo_name.as_deref(),
            config.issue_number,
        ) {
            operations.push(build_content_lookup_query(owner, repo, number));
        } else {
            return Err(CanopusError::InvalidInput(
                "GitHub Project sync requires github_owner, github_repo, and github_issue_number when github_project_item_id is absent".to_string(),
            ));
        }
        "addProjectV2ItemById".to_string()
    };

    let status_field_source = if config.status_field_id.is_some() {
        "field_id".to_string()
    } else {
        operations.push(build_project_fields_query(
            project_id_for_known_ops.as_deref().unwrap_or("PROJECT_ID"),
        ));
        "field_name".to_string()
    };
    let status_option_source = if config.status_option_id.is_some() {
        "option_id".to_string()
    } else if config.status_option_name.is_some() {
        "option_name".to_string()
    } else {
        "github_project_status".to_string()
    };

    if matches!(
        config.mode,
        GitHubProjectMode::DryRunOffline | GitHubProjectMode::MutateLive
    ) {
        let project_id = project_id_for_known_ops.as_deref().unwrap_or("PROJECT_ID");
        if config.project_item_id.is_none() {
            operations.push(build_add_project_item_mutation(project_id, "CONTENT_ID"));
        }
        let item_id = config
            .project_item_id
            .as_deref()
            .unwrap_or("PROJECT_ITEM_ID");
        let field_id = config
            .status_field_id
            .as_deref()
            .unwrap_or("STATUS_FIELD_ID");
        let option_id = config
            .status_option_id
            .as_deref()
            .unwrap_or("STATUS_OPTION_ID");
        operations.push(build_update_project_status_mutation(
            project_id, item_id, field_id, option_id,
        ));
    }

    if matches!(config.mode, GitHubProjectMode::ValidateReadOnly)
        && operations
            .iter()
            .any(|operation| operation.kind == GraphQlOperationKind::Mutation)
    {
        return Err(CanopusError::InvalidInput(
            "validate-read-only mode must not construct GitHub Project mutations".to_string(),
        ));
    }

    Ok(GitHubProjectSyncPlan {
        mode: config.mode,
        project_id_source,
        item_id_source,
        status_field_source,
        status_option_source,
        operations,
    })
}

fn enforce_project_gates(mode: GitHubProjectMode, gates: &GitHubProjectGates) -> CanopusResult<()> {
    match mode {
        GitHubProjectMode::DryRunOffline => Ok(()),
        GitHubProjectMode::ValidateReadOnly if gates.enable_github => Ok(()),
        GitHubProjectMode::ValidateReadOnly => Err(CanopusError::InvalidInput(
            "validate-read-only requires CANOPUS_ENABLE_GITHUB=1 before any GitHub Project HTTP call".to_string(),
        )),
        GitHubProjectMode::MutateLive
            if gates.enable_github && gates.enable_live_mutations && gates.allow_project_mutation =>
        {
            Ok(())
        }
        GitHubProjectMode::MutateLive => Err(CanopusError::InvalidInput(
            "mutate-live requires CANOPUS_ENABLE_GITHUB=1, CANOPUS_ENABLE_LIVE_MUTATIONS=1, and CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1 before any GitHub Project mutation".to_string(),
        )),
    }
}

fn project_lookup_parts(
    config: &GitHubProjectSyncConfig,
) -> CanopusResult<(ProjectOwnerKind, String, u64)> {
    if let Some(url) = config
        .project_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
    {
        let parts = parse_project_url(url)?;
        return Ok((parts.owner_kind, parts.owner, parts.number));
    }

    let owner_kind = config.project_owner_kind.ok_or_else(|| {
        CanopusError::InvalidInput(
            "GitHub Project sync requires GITHUB_PROJECT_ID, GITHUB_PROJECT_URL, or GITHUB_PROJECT_OWNER_KIND".to_string(),
        )
    })?;
    let owner = config
        .project_owner
        .clone()
        .filter(|owner| !owner.trim().is_empty())
        .ok_or_else(|| {
            CanopusError::InvalidInput(
                "GitHub Project sync requires GITHUB_PROJECT_OWNER when project ID/URL is absent"
                    .to_string(),
            )
        })?;
    let number = config.project_number.ok_or_else(|| {
        CanopusError::InvalidInput(
            "GitHub Project sync requires GITHUB_PROJECT_NUMBER when project ID/URL is absent"
                .to_string(),
        )
    })?;
    Ok((owner_kind, owner, number))
}

pub fn resolve_single_select_field_option(
    fields_response: &Value,
    field_name: &str,
    option_name: &str,
) -> CanopusResult<(String, String)> {
    let nodes = fields_response
        .pointer("/data/node/fields/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| CanopusError::Tool("missing ProjectV2 fields nodes".to_string()))?;

    for node in nodes {
        if node.get("name").and_then(Value::as_str) == Some(field_name) {
            let field_id = node.get("id").and_then(Value::as_str).ok_or_else(|| {
                CanopusError::Tool(format!("missing ID for Project field `{field_name}`"))
            })?;
            let options = node
                .get("options")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    CanopusError::Tool(format!(
                        "Project field `{field_name}` is not a single-select field with options"
                    ))
                })?;
            for option in options {
                if option.get("name").and_then(Value::as_str) == Some(option_name) {
                    let option_id = option.get("id").and_then(Value::as_str).ok_or_else(|| {
                        CanopusError::Tool(format!(
                            "missing ID for Project option `{option_name}` on field `{field_name}`"
                        ))
                    })?;
                    return Ok((field_id.to_string(), option_id.to_string()));
                }
            }
            return Err(CanopusError::Tool(format!(
                "Project field `{field_name}` does not contain option `{option_name}`"
            )));
        }
    }

    Err(CanopusError::Tool(format!(
        "Project field `{field_name}` was not found"
    )))
}

fn project_content_lookup_inputs(
    config: &GitHubProjectSyncConfig,
) -> CanopusResult<(&str, &str, u64)> {
    let owner = config.repo_owner.as_deref().ok_or_else(|| {
        CanopusError::InvalidInput(
            "GitHub Project content lookup requires github_owner".to_string(),
        )
    })?;
    let repo = config.repo_name.as_deref().ok_or_else(|| {
        CanopusError::InvalidInput("GitHub Project content lookup requires github_repo".to_string())
    })?;
    let number = config.issue_number.ok_or_else(|| {
        CanopusError::InvalidInput(
            "GitHub Project content lookup requires github_issue_number".to_string(),
        )
    })?;
    Ok((owner, repo, number))
}

fn requested_status_field_name(config: &GitHubProjectSyncConfig) -> &str {
    config
        .status_field_name
        .as_deref()
        .filter(|field| !field.trim().is_empty())
        .unwrap_or("Status")
}

fn requested_status_option_name<'a>(
    config: &'a GitHubProjectSyncConfig,
    context: &str,
) -> CanopusResult<&'a str> {
    config
        .status_option_name
        .as_deref()
        .filter(|option| !option.trim().is_empty())
        .or_else(|| {
            config
                .status
                .as_deref()
                .filter(|status| !status.trim().is_empty())
        })
        .ok_or_else(|| {
            CanopusError::InvalidInput(format!(
                "GitHub Project {context} requires github_project_status_option_name or github_project_status when option ID is absent"
            ))
        })
}

fn parse_project_lookup_id(
    response: &Value,
    owner_kind: ProjectOwnerKind,
) -> CanopusResult<String> {
    let pointer = match owner_kind {
        ProjectOwnerKind::User => "/data/user/projectV2/id",
        ProjectOwnerKind::Org => "/data/organization/projectV2/id",
    };
    response
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CanopusError::Tool("missing ProjectV2 id in GraphQL response".to_string()))
}

fn parse_content_lookup_id(response: &Value) -> CanopusResult<String> {
    response
        .pointer("/data/repository/issue/id")
        .and_then(Value::as_str)
        .or_else(|| {
            response
                .pointer("/data/repository/pullRequest/id")
                .and_then(Value::as_str)
        })
        .map(str::to_string)
        .ok_or_else(|| {
            CanopusError::Tool(
                "missing issue or pull request content id in GraphQL response".to_string(),
            )
        })
}

fn parse_added_project_item_id(response: &Value) -> CanopusResult<String> {
    response
        .pointer("/data/addProjectV2ItemById/item/id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            CanopusError::Tool(
                "missing ProjectV2 item id in addProjectV2ItemById response".to_string(),
            )
        })
}
