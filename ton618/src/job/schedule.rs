
use std::time::Duration;
use chrono::Utc;
use cron::Schedule as CronExpr;
use std::str::FromStr;

pub enum Schedule {
    Fixed(Duration),
    Cron(CronExpr)
}

impl Schedule {
    pub fn fixed(interval: Duration) -> Self {
        Schedule::Fixed(interval)
    }

    pub fn cron(expr: &str) -> anyhow::Result<Self> {
        let cron_expr = CronExpr::from_str(expr)?;

        Ok(Schedule::Cron(cron_expr))
    }

    pub fn next_delay(&self) -> Duration {
        match self {
            Schedule::Fixed(interval) => *interval,
            Schedule::Cron(expr) => {
                let now = Utc::now();
                expr.upcoming(Utc)
                    .next()
                    .map(|next| {
                        let delta = next - now;              // chrono::Duration
                        delta.to_std().unwrap_or_default()   // std::time::Duration
                    })
                    .unwrap_or_default()                   // Option<Duration> → Duration
            }
        }
    }
}
