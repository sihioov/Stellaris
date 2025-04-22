// job/traits.rs

use std::time::Instant;
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait Job: Send + Sync {
    fn name(&self) -> &'static str;

    //fn next_due(&self) -> Instant;

    async fn execute(&mut self) -> Result<()>;
}

