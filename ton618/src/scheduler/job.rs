//! scheduler/traits.rs

use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

#[async_trait]
pub trait Job: Send + Sync {
    fn name(&self) -> Cow<'static, str>;
    async fn execute(&mut self) -> Result<()>;
    fn max_retries(&self) -> usize {
        0
    }
}
