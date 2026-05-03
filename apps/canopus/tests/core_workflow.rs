use canopus::core::{
    Agenda, AgentRole, AgentTask, ArtifactKind, GitHubProjectMetadata, GitHubProjectMode,
    WorkflowState,
};

#[test]
fn agenda_rejects_empty_request() {
    let result = Agenda::new_with_id("CANOPUS-1", "   ");
    assert!(result.is_err());
}

#[test]
fn agenda_creates_planner_task() {
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-1", &agenda, AgentRole::Planner);

    assert_eq!(task.id, "TASK-1");
    assert_eq!(task.agenda_id, "CANOPUS-1");
    assert_eq!(task.role, AgentRole::Planner);
    assert!(task.prompt.contains("add tests"));
}

#[test]
fn workflow_allows_local_patch_path() {
    let state = WorkflowState::Created
        .transition_to(WorkflowState::Planned)
        .unwrap()
        .transition_to(WorkflowState::Executing)
        .unwrap()
        .transition_to(WorkflowState::Checking)
        .unwrap()
        .transition_to(WorkflowState::Reviewed)
        .unwrap()
        .transition_to(WorkflowState::Completed)
        .unwrap();

    assert_eq!(state, WorkflowState::Completed);
}

#[test]
fn workflow_rejects_skipping_plan() {
    let err = WorkflowState::Created
        .transition_to(WorkflowState::Executing)
        .unwrap_err();

    assert!(err.to_string().contains("Created -> Executing"));
}

#[test]
fn artifact_kind_has_stable_file_names() {
    assert_eq!(ArtifactKind::Plan.file_name(), "plan.md");
    assert_eq!(ArtifactKind::Diff.file_name(), "diff.md");
    assert_eq!(ArtifactKind::TestResult.file_name(), "test-result.md");
    assert_eq!(ArtifactKind::Review.file_name(), "review.md");
    assert_eq!(ArtifactKind::RuntimeLog.file_name(), "runtime-log.md");
}

#[test]
fn agent_task_backend_payload_includes_github_project_metadata() {
    let agenda = Agenda::new_with_id("CANOPUS-1", "sync project").unwrap();
    let task = AgentTask::for_agenda_with_all_metadata(
        "TASK-1",
        &agenda,
        AgentRole::Coder,
        "agent",
        Some("UPSTREAM-1".to_string()),
        None,
        Some(GitHubProjectMetadata {
            id: Some("PVT_project".to_string()),
            url: Some("https://github.com/orgs/acme/projects/1".to_string()),
            item_id: Some("PVTI_existing".to_string()),
            status: Some("Ready".to_string()),
            owner_kind: Some("org".to_string()),
            owner: Some("acme".to_string()),
            number: Some(1),
            status_field_id: Some("PVTSSF_status".to_string()),
            status_field_name: Some("Status".to_string()),
            status_option_id: Some("ready".to_string()),
            status_option_name: Some("Ready".to_string()),
            mode: Some(GitHubProjectMode::DryRunOffline),
        }),
    );

    let payload = task.to_backend_payload();

    assert!(payload.contains("github_project_id=PVT_project"));
    assert!(payload.contains("github_project_owner_kind=org"));
    assert!(payload.contains("github_project_number=1"));
    assert!(payload.contains("github_project_status_field_name=Status"));
    assert!(payload.contains("github_project_status_option_name=Ready"));
    assert!(payload.contains("github_project_mode=dry-run-offline"));
}
