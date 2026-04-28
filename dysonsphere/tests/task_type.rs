use dysonsphere::message::TaskType;

#[test]
fn news_a_serialization_stays_compatible() {
    let json = serde_json::to_string(&TaskType::NewsA).unwrap();
    assert_eq!(json, "\"NewsA\"");
}

#[test]
fn custom_task_type_can_represent_application_workloads() {
    let task_type = TaskType::Custom("canopus.agent".to_string());
    let json = serde_json::to_string(&task_type).unwrap();
    let decoded: TaskType = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, task_type);
}

#[test]
fn pending_proposal_status_serializes_stably() {
    let json = serde_json::to_string(&dysonsphere::status::TaskStatus::PendingProposal).unwrap();
    assert_eq!(json, "\"PendingProposal\"");
}
