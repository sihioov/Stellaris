//! scheduler/schedule.rs

#[cfg(feature = "scheduler-cron")]
use chrono::Utc;
#[cfg(feature = "scheduler-cron")]
use cron::Schedule as CronExpr;
#[cfg(feature = "scheduler-cron")]
use std::str::FromStr;
use std::time::Duration;

pub enum Schedule {
    Fixed(Duration),
    #[allow(dead_code)]
    #[cfg(feature = "scheduler-cron")]
    Cron(Box<CronExpr>),
}

impl Schedule {
    pub fn fixed(interval: Duration) -> Self {
        Schedule::Fixed(interval)
    }

    #[allow(dead_code)]
    #[cfg(feature = "scheduler-cron")]
    pub fn cron(expr: &str) -> anyhow::Result<Self> {
        let cron_expr = CronExpr::from_str(expr)?;

        Ok(Schedule::Cron(Box::new(cron_expr)))
    }

    pub fn next_delay(&self) -> Duration {
        match self {
            Schedule::Fixed(interval) => *interval,
            #[cfg(feature = "scheduler-cron")]
            Schedule::Cron(expr) => {
                let now = Utc::now();
                expr.as_ref()
                    .upcoming(Utc)
                    .next()
                    .map(|next| {
                        let delta = next - now; // chrono::Duration
                        delta.to_std().unwrap_or_default() // std::time::Duration
                    })
                    .unwrap_or_default() // Option<Duration> → Duration
            }
        }
    }
}
