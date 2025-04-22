use std::sync::Mutex;
use std::collections::VecDeque;
use crate::job::traits::Job;

pub trait JobQueue<J: Job>: Send + Sync {
    fn enqueue(&mut self, job: J);
    fn dequeue(&mut self) -> Option<J>;
}

pub struct InMemoryJobQueue<J: Job> {
    inner: Mutex<VecDeque<J>>,
}

impl<J: Job> InMemoryJobQueue<J> {
    pub fn new() -> Self {
        InMemoryJobQueue {
            inner: Mutex::new(VecDeque::new()),
        }
    }
}

impl<J: Job> JobQueue<J> for InMemoryJobQueue<J> {
    fn enqueue(&mut self, job: J) {
        let mut queue = self.inner.lock().unwrap();
        queue.push_back(job);
    }

    fn dequeue(&mut self) -> Option<J> {
        let mut queue = self.inner.lock().unwrap();
        queue.pop_front()
    }
}