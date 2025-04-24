//! scheduler/queue.rs
//!
//! In-memory scheduler queue using a min-heap (priority queue) for ScheduledJob,
//! where jobs are ordered by next_run Instant (earliest first).

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Mutex;
use std::time::Instant;
use crate::scheduler::job::Job;
use crate::scheduler::schedule::Schedule;

/// A job scheduled with its next execution time.
pub struct ScheduledJob<J: Job> {
    pub next_run: Instant,
    pub job: J,
    pub schedule: Schedule,
}

impl<J: Job> ScheduledJob<J> {
    /// Create a new ScheduledJob with next_run = now + schedule.next_delay()
    pub fn new(job: J, schedule: Schedule) -> Self {
        let next_run = Instant::now() + schedule.next_delay();
        ScheduledJob { next_run, job, schedule }
    }

    /// Update the next_run based on the schedule.
    pub fn update_next_run(&mut self) {
        self.next_run = Instant::now() + self.schedule.next_delay();
    }
}

// Implement ordering for min-heap: smallest next_run has highest priority.
impl<J: Job> Ord for ScheduledJob<J> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse to make a min-heap out of BinaryHeap (which is max-heap).
        other.next_run.cmp(&self.next_run)
    }
}

impl<J: Job> PartialOrd for ScheduledJob<J> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<J: Job> PartialEq for ScheduledJob<J> {
    fn eq(&self, other: &Self) -> bool {
        self.next_run == other.next_run
    }
}

impl<J: Job> Eq for ScheduledJob<J> {}

/// JobQueue trait specialized for ScheduledJob<J>.
pub trait JobQueue<J: Job>: Send + Sync {
    /// Enqueue a scheduled job into the priority queue.
    fn enqueue(&self, job: ScheduledJob<J>);
    /// Dequeue the scheduled job with the earliest next_run.
    fn dequeue(&self) -> Option<ScheduledJob<J>>;
    /// Peek the next_run time of the highest priority job without removing it.
    fn peek_next_run(&self) -> Option<Instant>;
}

/// In-memory priority queue implementation for ScheduledJob.
pub struct InMemoryJobQueue<J: Job> {
    inner: Mutex<BinaryHeap<ScheduledJob<J>>>,
}

impl<J: Job> InMemoryJobQueue<J> {
    pub fn new() -> Self {
        InMemoryJobQueue {
            inner: Mutex::new(BinaryHeap::new()),
        }
    }
}

impl<J: Job> JobQueue<J> for InMemoryJobQueue<J> {
    fn enqueue(&self, job: ScheduledJob<J>) {
        let mut heap = self.inner.lock().unwrap();
        heap.push(job);
    }

    fn dequeue(&self) -> Option<ScheduledJob<J>> {
        let mut heap = self.inner.lock().unwrap();
        heap.pop()
    }

    fn peek_next_run(&self) -> Option<Instant> {
        let heap = self.inner.lock().unwrap();
        // Peek returns an Option<&ScheduledJob>, map it to Option<Instant>
        heap.peek().map(|scheduled_job| scheduled_job.next_run)
    }
}
