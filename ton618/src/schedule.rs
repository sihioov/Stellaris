use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

pub enum ScheduleType {
    Fixed(Duration),
    Cron(Schedule),
}

pub struct Scheduler {
    schedule_type: ScheduleType,
}

impl Scheduler {
    pub fn fixed(interval: Duration) -> Self {
        Self {
            schedule_type: ScheduleType::Fixed(interval),
        }
    }

    pub fn cron(expr: &str) -> anyhow::Result<Self> {
        let schedule = Schedule::from_str(expr)?;
        Ok(Self {
            schedule_type: ScheduleType::Cron(schedule),
        })
    }

    pub async fn wait_for_next(&self) {
        match &self.schedule_type {
            ScheduleType::Fixed(duration) => {
                sleep(*duration).await;
            }
            ScheduleType::Cron(schedule) => {
                let now = Utc::now();
                if let Some(next) = schedule.upcoming(Utc).next() {
                    let wait = next - now;
                    if let Ok(wait_std) = wait.to_std() {
                        sleep(wait_std).await;
                    }
                }
            }
        }
    }
}
