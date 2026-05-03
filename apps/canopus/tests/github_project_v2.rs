use canopus::adapters::github::{
    build_add_project_item_mutation, build_content_lookup_query, build_project_fields_query,
    build_project_lookup_query, build_project_sync_plan, build_update_project_status_mutation,
    parse_project_url, resolve_single_select_field_option, GitHubProjectGates,
    GitHubProjectSyncConfig, GraphQlOperationKind, ProjectOwnerKind,
};
use canopus::core::GitHubProjectMode;
use serde_json::Value;

fn live_gates() -> GitHubProjectGates {
    GitHubProjectGates {
        enable_github: true,
        enable_live_mutations: true,
        allow_project_mutation: true,
    }
}

fn base_config(mode: GitHubProjectMode) -> GitHubProjectSyncConfig {
    GitHubProjectSyncConfig {
        mode,
        project_id: Some("PVT_project".to_string()),
        repo_owner: Some("acme".to_string()),
        repo_name: Some("demo".to_string()),
        issue_number: Some(7),
        status_field_id: Some("PVTSSF_status".to_string()),
        status_option_id: Some("ready".to_string()),
        status: Some("Ready".to_string()),
        ..GitHubProjectSyncConfig::default()
    }
}

#[test]
fn github_project_dry_run_offline_zero_http() {
    let plan = build_project_sync_plan(
        &base_config(GitHubProjectMode::DryRunOffline),
        &GitHubProjectGates::default(),
    )
    .unwrap();

    assert_eq!(plan.mode, GitHubProjectMode::DryRunOffline);
    assert_eq!(plan.project_id_source, "node_id");
    assert!(plan
        .operations
        .iter()
        .any(|operation| operation.operation_name == "AddProjectV2ItemById"));
    assert!(plan
        .operations
        .iter()
        .any(|operation| operation.operation_name == "UpdateProjectV2ItemFieldValue"));
}

#[test]
fn github_project_validate_read_only_queries_only() {
    let config = GitHubProjectSyncConfig {
        mode: GitHubProjectMode::ValidateReadOnly,
        project_url: Some("https://github.com/orgs/acme/projects/1".to_string()),
        repo_owner: Some("acme".to_string()),
        repo_name: Some("demo".to_string()),
        issue_number: Some(7),
        status_field_name: Some("Status".to_string()),
        status_option_name: Some("Ready".to_string()),
        ..GitHubProjectSyncConfig::default()
    };

    let plan = build_project_sync_plan(
        &config,
        &GitHubProjectGates {
            enable_github: true,
            ..GitHubProjectGates::default()
        },
    )
    .unwrap();

    assert!(!plan.operations.is_empty());
    assert!(plan
        .operations
        .iter()
        .all(|operation| operation.kind == GraphQlOperationKind::Query));
}

#[test]
fn github_project_missing_gate_fails_before_http() {
    let validate_err = build_project_sync_plan(
        &base_config(GitHubProjectMode::ValidateReadOnly),
        &GitHubProjectGates::default(),
    )
    .unwrap_err();
    assert!(validate_err.to_string().contains("CANOPUS_ENABLE_GITHUB=1"));

    for gates in [
        GitHubProjectGates::default(),
        GitHubProjectGates {
            enable_github: true,
            ..GitHubProjectGates::default()
        },
        GitHubProjectGates {
            enable_github: true,
            enable_live_mutations: true,
            allow_project_mutation: false,
        },
    ] {
        let err = build_project_sync_plan(&base_config(GitHubProjectMode::MutateLive), &gates)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1"));
    }
}

#[test]
fn github_project_existing_item_id_skips_add() {
    let config = GitHubProjectSyncConfig {
        project_item_id: Some("PVTI_existing".to_string()),
        ..base_config(GitHubProjectMode::MutateLive)
    };

    let plan = build_project_sync_plan(&config, &live_gates()).unwrap();

    assert_eq!(plan.item_id_source, "provided");
    assert!(!plan
        .operations
        .iter()
        .any(|operation| operation.operation_name == "AddProjectV2ItemById"));
    let update = plan
        .operations
        .iter()
        .find(|operation| operation.operation_name == "UpdateProjectV2ItemFieldValue")
        .unwrap();
    assert_eq!(update.variables["item"], "PVTI_existing");
}

#[test]
fn github_project_add_before_update_order() {
    let plan = build_project_sync_plan(&base_config(GitHubProjectMode::MutateLive), &live_gates())
        .unwrap();
    let names: Vec<&str> = plan
        .operations
        .iter()
        .map(|operation| operation.operation_name)
        .collect();

    let content = names
        .iter()
        .position(|name| *name == "ProjectV2ContentLookup")
        .unwrap();
    let add = names
        .iter()
        .position(|name| *name == "AddProjectV2ItemById")
        .unwrap();
    let update = names
        .iter()
        .position(|name| *name == "UpdateProjectV2ItemFieldValue")
        .unwrap();
    assert!(content < add && add < update);
}

#[test]
fn github_project_id_node_id_skips_lookup() {
    let config = GitHubProjectSyncConfig {
        mode: GitHubProjectMode::ValidateReadOnly,
        project_id: Some("PVT_project".to_string()),
        project_item_id: Some("PVTI_existing".to_string()),
        status_field_name: Some("Status".to_string()),
        status_option_name: Some("Ready".to_string()),
        ..GitHubProjectSyncConfig::default()
    };

    let plan = build_project_sync_plan(
        &config,
        &GitHubProjectGates {
            enable_github: true,
            ..GitHubProjectGates::default()
        },
    )
    .unwrap();

    assert_eq!(plan.project_id_source, "node_id");
    assert!(!plan
        .operations
        .iter()
        .any(
            |operation| operation.operation_name == "ProjectV2UserLookup"
                || operation.operation_name == "ProjectV2OrgLookup"
        ));
}

#[test]
fn github_project_url_parses_owner_number() {
    let org = parse_project_url("https://github.com/orgs/acme/projects/42").unwrap();
    assert_eq!(org.owner_kind, ProjectOwnerKind::Org);
    assert_eq!(org.owner, "acme");
    assert_eq!(org.number, 42);

    let user = parse_project_url("https://github.com/users/octo/projects/3/").unwrap();
    assert_eq!(user.owner_kind, ProjectOwnerKind::User);
    assert_eq!(user.owner, "octo");
    assert_eq!(user.number, 3);
}

#[test]
fn github_project_field_option_resolution_precedence() {
    let fixture: Value = serde_json::from_str(include_str!(
        "fixtures/github_project_v2/project_fields_status.json"
    ))
    .unwrap();

    let (field_id, option_id) =
        resolve_single_select_field_option(&fixture, "Status", "Ready").unwrap();

    assert_eq!(field_id, "PVTSSF_status");
    assert_eq!(option_id, "ready");
}

#[test]
fn github_project_request_builders_use_expected_names_and_variables() {
    let project = build_project_lookup_query(ProjectOwnerKind::Org, "acme", 1);
    assert_eq!(project.operation_name, "ProjectV2OrgLookup");
    assert_eq!(project.variables["owner"], "acme");
    assert_eq!(project.variables["number"], 1);

    let content = build_content_lookup_query("acme", "demo", 7);
    assert_eq!(content.operation_name, "ProjectV2ContentLookup");
    assert_eq!(content.variables["repo"], "demo");

    let fields = build_project_fields_query("PVT_project");
    assert_eq!(fields.operation_name, "ProjectV2Fields");
    assert_eq!(fields.variables["project"], "PVT_project");

    let add = build_add_project_item_mutation("PVT_project", "I_issue_7");
    assert_eq!(add.operation_name, "AddProjectV2ItemById");
    assert_eq!(add.variables["content"], "I_issue_7");

    let update = build_update_project_status_mutation(
        "PVT_project",
        "PVTI_existing",
        "PVTSSF_status",
        "ready",
    );
    assert_eq!(update.operation_name, "UpdateProjectV2ItemFieldValue");
    assert_eq!(update.variables["option"], "ready");
}
