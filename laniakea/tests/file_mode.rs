use dysonsphere::{
    db::{task_table_file::FileTaskTable, TaskTable},
    message::{TaskMessage, TaskMeta, TaskType},
    status::TaskStatus,
};
use laniakea::worker::run_file_loop;
use std::sync::Arc;
use std::time::Duration;

fn make_dispatched_task(id: &str, task_type: TaskType) -> TaskMessage {
    TaskMessage {
        task_id: id.to_string(),
        task_type,
        payload: format!("payload-{id}"),
        meta: TaskMeta {
            status: TaskStatus::Dispatched,
            ..TaskMeta::default()
        },
    }
}

fn make_pending_proposal_task(id: &str) -> TaskMessage {
    TaskMessage {
        task_id: id.to_string(),
        task_type: TaskType::Bug,
        payload: format!("payload-{id}"),
        meta: TaskMeta {
            status: TaskStatus::PendingProposal,
            ..TaskMeta::default()
        },
    }
}

#[tokio::test]
async fn file_mode_processes_dispatched_tasks_to_pending_review() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let tasks = vec![
        make_dispatched_task("T1", TaskType::NewsA),
        make_dispatched_task("T2", TaskType::NewsA),
    ];
    let json = serde_json::to_string(&tasks).unwrap();
    std::fs::write(file.path(), &json).unwrap();

    let table = Arc::new(FileTaskTable::new(file.path().to_path_buf()));

    let result = tokio::time::timeout(
        Duration::from_secs(3),
        run_file_loop(Arc::clone(&table), Duration::from_millis(100)),
    )
    .await;

    // 루프는 무한이므로 timeout Err가 정상
    assert!(result.is_err(), "expected timeout, worker exited early");

    let t1 = table.fetch("T1").await.unwrap().unwrap();
    let t2 = table.fetch("T2").await.unwrap().unwrap();
    assert_eq!(
        t1.meta.status,
        TaskStatus::PendingReview,
        "T1 should be PendingReview"
    );
    assert_eq!(
        t2.meta.status,
        TaskStatus::PendingReview,
        "T2 should be PendingReview"
    );

    // idempotency: PendingReview는 재처리되지 않음
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t1_again = table.fetch("T1").await.unwrap().unwrap();
    assert_eq!(
        t1_again.meta.status,
        TaskStatus::PendingReview,
        "T1 must not be re-dispatched"
    );
}

#[tokio::test]
async fn file_mode_does_not_process_pending_proposals() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let tasks = vec![make_pending_proposal_task("P1")];
    let json = serde_json::to_string(&tasks).unwrap();
    std::fs::write(file.path(), &json).unwrap();

    let table = Arc::new(FileTaskTable::new(file.path().to_path_buf()));

    let result = tokio::time::timeout(
        Duration::from_millis(250),
        run_file_loop(Arc::clone(&table), Duration::from_millis(50)),
    )
    .await;

    assert!(result.is_err(), "expected timeout, worker exited early");
    let task = table.fetch("P1").await.unwrap().unwrap();
    assert_eq!(task.meta.status, TaskStatus::PendingProposal);
}
