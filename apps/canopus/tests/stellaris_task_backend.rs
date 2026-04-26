use canopus::adapters::task_backend::StellarisTaskBackend;
use canopus::core::{Agenda, AgentRole, AgentTask};
use canopus::ports::TaskBackend;
use dysonsphere::db::{FileTaskTable, TaskTable};
use dysonsphere::message::TaskType;
use std::fs;

fn test_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("canopus-{name}-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

#[test]
fn submits_agent_task_as_stellaris_task_message() {
    let path = test_file("stellaris-backend");
    let backend = StellarisTaskBackend::new(path.clone()).unwrap();
    let agenda = Agenda::new_with_id("CANOPUS-1", "add tests").unwrap();
    let task = AgentTask::for_agenda("TASK-1", &agenda, AgentRole::Coder);

    let submitted = backend.submit(&task).unwrap();

    assert_eq!(submitted.backend_id, "TASK-1");

    let table = FileTaskTable::new(path.clone());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let stored = runtime.block_on(table.fetch("TASK-1")).unwrap().unwrap();

    assert_eq!(stored.task_id, "TASK-1");
    assert_eq!(
        stored.task_type,
        TaskType::Custom("canopus.agent".to_string())
    );
    assert!(stored.payload.contains("role=coder"));
    assert!(stored.payload.contains("add tests"));
    let _ = fs::remove_file(path);
}
