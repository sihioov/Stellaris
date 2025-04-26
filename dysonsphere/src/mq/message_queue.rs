/// dysonsphere/src/mq/message_queue.rs

use crate::message::TaskMessage;
use tokio::sync::mpsc::Receiver;
use async_trait::async_trait;

#[async_trait]
pub trait MessageQueue: Send + Sync + 'static {
    type Error;

    async fn publish(
        &self,
        topic: &str,
        message: TaskMessage,
    ) -> std::result::Result<(), Self::Error>;

    async fn subscribe(
        &self,
        topic: &str,
    ) -> std::result::Result<Receiver<TaskMessage>, Self::Error>;
}
