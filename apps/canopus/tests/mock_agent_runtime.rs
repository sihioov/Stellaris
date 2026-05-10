use canopus::adapters::agent_runtime::MockAgentRuntime;
use canopus::core::{
    Agenda, AgentMessage, AgentRole, AgentRunResult, AgentTask, Artifact, ArtifactKind,
};
use canopus::ports::{AgentContext, AgentRuntime};
use chrono::{TimeZone, Utc};
use std::fs;

fn test_repo(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("canopus-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test]
async fn planner_returns_plan_artifact() {
    let repo = test_repo("mock-planner");
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-PLAN", &agenda, AgentRole::Planner);
    let runtime = MockAgentRuntime;

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &[],
        )
        .await
        .unwrap();

    assert_eq!(result.task_id, "TASK-PLAN");
    assert_eq!(result.artifacts[0].kind, ArtifactKind::Plan);
    assert!(result.artifacts[0].content.contains("Mock plan"));
    assert!(result.message_log.is_empty());
    let _ = fs::remove_dir_all(repo);
}

#[tokio::test]
async fn coder_runtime_creates_a_repo_file_for_diff_testing() {
    let repo = test_repo("mock-coder");
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-CODE", &agenda, AgentRole::Coder);
    let runtime = MockAgentRuntime;

    let result = runtime
        .run(
            &task,
            &AgentContext {
                repo_path: repo.clone(),
            },
            &[],
        )
        .await
        .unwrap();

    assert!(repo.join("canopus-mock-output.txt").exists());
    assert_eq!(result.artifacts[0].kind, ArtifactKind::RuntimeLog);
    assert!(result.message_log.is_empty());
    let _ = fs::remove_dir_all(repo);
}

#[test]
fn agent_run_result_deserializes_legacy_records_with_empty_message_log() {
    let legacy = serde_json::json!({
        "task_id": "TASK-LEGACY",
        "summary": "legacy result",
        "artifacts": [{
            "task_id": "TASK-LEGACY",
            "kind": "RuntimeLog",
            "content": "old runtime output"
        }]
    });

    let result: AgentRunResult = serde_json::from_value(legacy).unwrap();

    assert_eq!(result.task_id, "TASK-LEGACY");
    assert_eq!(result.artifacts[0].kind, ArtifactKind::RuntimeLog);
    assert!(result.message_log.is_empty());
}

#[test]
fn agent_run_result_round_trips_message_log_schema() {
    let created_at = Utc.with_ymd_and_hms(2026, 5, 5, 0, 0, 0).unwrap();
    let result = AgentRunResult {
        task_id: "TASK-MSG".to_string(),
        summary: "result with messages".to_string(),
        artifacts: vec![Artifact {
            task_id: "TASK-MSG".to_string(),
            kind: ArtifactKind::Review,
            content: "review output".to_string(),
        }],
        message_log: vec![AgentMessage {
            role: "planner".to_string(),
            content: "plan drafted".to_string(),
            created_at,
        }],
        token_usage: None,
    };

    let encoded = serde_json::to_string(&result).unwrap();
    let decoded: AgentRunResult = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, result);
    assert_eq!(decoded.message_log[0].created_at, created_at);
}
