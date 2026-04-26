use canopus::adapters::agent_runtime::MockAgentRuntime;
use canopus::core::{Agenda, AgentRole, AgentTask, ArtifactKind};
use canopus::ports::{AgentContext, AgentRuntime};
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
    let _ = fs::remove_dir_all(repo);
}
