//! scheduler/runner.rs
//!
//! Runner: PriorityQueue based scheduler loop impl, using peek.

use crate::scheduler::job::Job;
use crate::scheduler::queue::JobQueue;
use std::marker::PhantomData;
use std::time::{Duration, Instant};
use tokio::time::sleep;

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
            let mut sleep_duration = Duration::from_millis(100); // Default sleep if queue is empty or next job is far away

            if let Some(next_run_time) = self.queue.peek_next_run() {
                let now = Instant::now();

                if next_run_time <= now {
                    /// Time to run the job
                    if let Some(mut schedule_job) = self.queue.dequeue() {
                        /// Execute the job
                        if let Err(e) = schedule_job.job.execute().await {
                            /// error: retry, drop, log, etc.
                            eprintln!("Job '{}' error: {:?}", schedule_job.job.name(), e);
                        }

                        schedule_job.update_next_run();
                        self.queue.enqueue(schedule_job);
                    }
                    continue; //< Skip the sleep and re-evaluate the queue top
                } else {
                    /// Wait until the next jobs scheduled time
                    sleep_duration = next_run_time.saturating_duration_since(now);
                }
            }
            /// If queue is empty or next job is in the future, sleep.
            sleep(sleep_duration).await;
        }
    }
}


