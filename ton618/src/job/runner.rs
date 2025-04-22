use crate::job::traits::Job;
use crate::job::schedule::Schedule;
use crate::job::queue::JobQueue;
use std::marker::PhantomData;
use tokio::time::sleep;

pub struct Runner<J, Q>
where
    J: Job,
    Q: JobQueue<J>,
{
    queue: Q,
    schedule: Schedule,
    _marker: PhantomData<J>,
}

impl<J, Q> Runner<J, Q>
where
    J: Job,
    Q: JobQueue<J>,
{
    pub fn new(queue: Q, schedule: Schedule) -> Self {
        Runner { queue, schedule, _marker: PhantomData }
    }

    pub async fn run(&mut self) {
        loop {
            if let Some(mut job) = self.queue.dequeue() {
                let name = job.name();
                if let Err(e) = job.execute().await {
                    eprintln!("Job '{}' error {:?}", name, e);
                }
            }
            sleep(self.schedule.next_delay()).await
        }
    }
}