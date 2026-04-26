//! task/dispatcher.rs
//!
//! TaskDispatcher: 실행할 작업을 Worker에게 분배하는 컴포넌트

use crate::datasource::TaskDataSource;
use crate::file::FileDataSource;
use async_trait::async_trait;
use dysonsphere::error::{Result, StellarisError};
use dysonsphere::message::{TaskMessage, TaskType};
use dysonsphere::mq::rabbit_mq::RabbitMQClient;
use dysonsphere::mq::MessageQueue;
use dysonsphere::status::TaskStatus;
use std::collections::HashMap;
use std::sync::Arc;

/// Worker information and status
#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub instance_name: String,
    pub status: WorkerStatus,
    pub task_count: usize,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub task_types: Vec<TaskType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    Active,
    Busy,
    Inactive,
}

/// TaskDispatcher trait responsible for task distribution
#[async_trait]
pub trait TaskDispatcher: Send + Sync {
    /// Dispatch task to Worker
    async fn dispatch(&self, task: TaskMessage) -> Result<()>;

    /// Dispatch all tasks in the list
    async fn dispatch_all(&self, tasks: Vec<TaskMessage>) -> Result<()>;

    /// Check task execution result
    async fn check_task_status(&self, task_id: &str) -> Result<TaskStatus>;

    /// List workers and their status
    fn get_workers(&self) -> Result<Vec<WorkerInfo>>;
}

/// RabbitMQ를 사용하는 TaskDispatcher 구현체
pub struct RabbitMQTaskDispatcher {
    mq_client: Arc<RabbitMQClient>,
    worker_queues: HashMap<String, WorkerInfo>,
    current_worker_index: std::sync::atomic::AtomicUsize,
    data_source: Arc<FileDataSource>,
}

impl RabbitMQTaskDispatcher {
    /// Create RabbitMQTaskDispatcher
    pub fn new(mq_client: Arc<RabbitMQClient>, data_source: Arc<FileDataSource>) -> Self {
        RabbitMQTaskDispatcher {
            mq_client,
            worker_queues: HashMap::new(),
            current_worker_index: std::sync::atomic::AtomicUsize::new(0),
            data_source,
        }
    }

    /// Add worker with supported task types
    pub fn register_worker(&mut self, instance_name: String, task_types: Vec<TaskType>) {
        self.worker_queues.insert(
            instance_name.clone(),
            WorkerInfo {
                instance_name,
                status: WorkerStatus::Active,
                task_count: 0,
                last_seen: chrono::Utc::now(),
                task_types,
            },
        );
    }

    /// Remove worker
    pub fn unregister_worker(&mut self, instance_name: &str) -> bool {
        self.worker_queues.remove(instance_name).is_some()
    }

    /// Set worker status
    pub fn set_worker_status(&mut self, instance_name: &str, status: WorkerStatus) -> Result<()> {
        if let Some(worker) = self.worker_queues.get_mut(instance_name) {
            worker.status = status;
            worker.last_seen = chrono::Utc::now();
            Ok(())
        } else {
            Err(StellarisError::NotFound(format!(
                "Worker {} not found",
                instance_name
            )))
        }
    }

    /// Activate worker
    pub fn activate_worker(&mut self, instance_name: &str) -> Result<()> {
        self.set_worker_status(instance_name, WorkerStatus::Active)
    }

    /// Deactivate worker
    pub fn deactivate_worker(&mut self, instance_name: &str) -> Result<()> {
        self.set_worker_status(instance_name, WorkerStatus::Inactive)
    }

    /// Set worker as busy
    pub fn busy_worker(&mut self, instance_name: &str) -> Result<()> {
        self.set_worker_status(instance_name, WorkerStatus::Busy)
    }

    /// 작업 유형 기반 선택 후 라운드 로빈 적용
    fn select_target_queue(&self, task: &TaskMessage) -> Result<String> {
        // 작업 유형에 맞는 워커들 찾기
        let mut workers = Vec::new();

        for (instance_name, info) in &self.worker_queues {
            if info.status == WorkerStatus::Active && info.task_types.contains(&task.task_type) {
                workers.push(instance_name.clone());
            }
        }

        // 해당 유형의 워커들 중에서 라운드 로빈으로 선택
        if !workers.is_empty() {
            return self.round_robin(&workers);
        }

        // 작업 유형에 맞는 워커가 없으면 전체 워커 중에서 라운드 로빈
        self.default_round_robin()
    }

    /// 라운드 로빈
    fn round_robin(&self, workers: &[String]) -> Result<String> {
        if workers.is_empty() {
            return Err(StellarisError::MQError("No workers available".to_string()));
        }

        let index = self
            .current_worker_index
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            % workers.len();
        Ok(workers[index].clone())
    }

    /// 기본 라운드 로빈
    fn default_round_robin(&self) -> Result<String> {
        let worker_count = self.worker_queues.len();
        if worker_count == 0 {
            return Err(StellarisError::MQError(
                "No active workers available".to_string(),
            ));
        }

        // 활성 워커 목록 생성
        let active_workers: Vec<String> = self
            .worker_queues
            .iter()
            .filter(|(_, info)| info.status == WorkerStatus::Active)
            .map(|(name, _)| name.clone())
            .collect();

        self.round_robin(&active_workers)
    }
}

#[async_trait]
impl TaskDispatcher for RabbitMQTaskDispatcher {
    async fn dispatch(&self, task: TaskMessage) -> Result<()> {
        let queue = self.select_target_queue(&task)?;
        self.mq_client.publish(&queue, task).await
    }

    async fn dispatch_all(&self, tasks: Vec<TaskMessage>) -> Result<()> {
        for task in tasks {
            self.dispatch(task).await?;
        }

        Ok(())
    }

    async fn check_task_status(&self, task_id: &str) -> Result<TaskStatus> {
        match self.data_source.get_task(task_id).await {
            Ok(task) => Ok(task.meta.status),
            Err(err) => {
                let error_msg = format!("Failed to get task status: {}", err);
                if err.to_string().contains("not found") {
                    return Err(StellarisError::NotFound(format!(
                        "Task with id {} not found",
                        task_id
                    )));
                }
                Err(StellarisError::IoError(error_msg))
            }
        }
    }

    fn get_workers(&self) -> Result<Vec<WorkerInfo>> {
        Ok(self.worker_queues.values().cloned().collect())
    }
}

// Unit Test
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_distribution() {
        // TODO: 작업 유형 기반 분배 전략 테스트
    }
}
