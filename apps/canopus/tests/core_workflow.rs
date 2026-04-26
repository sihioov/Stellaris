use canopus::core::{Agenda, AgentRole, AgentTask, ArtifactKind, WorkflowState};

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
