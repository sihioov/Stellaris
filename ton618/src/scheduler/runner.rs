//! scheduler/runner.rs
//!
//! Runner: PriorityQueue based scheduler loop impl, using peek.

use crate::scheduler::job::Job;
use crate::scheduler::queue::JobQueue;
use std::marker::PhantomData;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use dysonsphere::error::StellarisError;

/// Runner: 주기적으로 ScheduledJob을 확인하고 실행/재등록하는 스케줄러 루프
pub struct Runner<J, Q>
where
    J: Job,
    Q: JobQueue<J>,
{
    queue: Q,
    _marker: PhantomData<J>,
}

impl<J, Q> Runner<J, Q>
where
    J: Job,
    Q: JobQueue<J>,
{
    /// Create new runner
    pub fn new(queue: Q) -> Self {
        Runner { queue, _marker: PhantomData }
    }

    /// Schedule loop using peek.
    pub async fn run(&self) {
        loop {
            // Default sleep if queue is empty or next job is far away
            let mut sleep_duration = Duration::from_millis(100);

            if let Some(next_run_time) = self.queue.peek_next_run() {
                let now = Instant::now();

                if next_run_time <= now {
                    // Time to run the job
                    if let Some(mut schedule_job) = self.queue.dequeue() {
                        // Execute the job
                        if let Err(e) = schedule_job.job.execute().await {
                            // 에러를 StellarisError로 변환하여 체계적으로 처리
                            let error = StellarisError::MQError(
                                format!("Job '{}' execution failed: {}", schedule_job.job.name(), e)
                            );
                            
                            // 체계적인 로깅 (실제 로깅 시스템과 연동 가능)
                            eprintln!("ERROR: {}", error);
                            
                            // 여기에 재시도 로직, 실패 보고 등의 추가 처리를 구현할 수 있음
                            // 예: retry_failed_job(&mut schedule_job, &error).await;
                        }

                        schedule_job.update_next_run();
                        self.queue.enqueue(schedule_job);
                    }
                    continue; //< Skip the sleep and re-evaluate the queue top
                } else {
                    // Wait until the next jobs scheduled time
                    sleep_duration = next_run_time.saturating_duration_since(now);
                }
            }
            // If queue is empty or next job is in the future, sleep.
            sleep(sleep_duration).await;
        }
    }
}


