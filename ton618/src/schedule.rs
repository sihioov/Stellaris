use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

pub enum ScheduleType {
    Fixed(Duration),            // 고정 간격: 10초마다 등
    Cron(Schedule),             // cron 표현식 기반: 매일 3시 등
}

pub struct Scheduler {
    schedule_type: ScheduleType,
}

impl Scheduler {
    /// 일정 주기
    pub fn fixed(interval: Duration) -> Self {
        Self {
            schedule_type: ScheduleType::Fixed(interval),
        }
    }

    /// cron 기반
    pub fn cron(expr: &str) -> anyhow::Result<Self> {
        let schedule = Schedule::from_str(expr)?;
        Ok(Self {
            schedule_type: ScheduleType::Cron(schedule),
        })
    }

    /// 다음 실행까지 대기
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
