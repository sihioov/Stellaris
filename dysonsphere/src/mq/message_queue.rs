/// dysonsphere/src/mq/message_queue.rs
use crate::message::TaskMessage;
use async_trait::async_trait;
use lapin::message::Delivery;
use tokio::sync::mpsc::Receiver;

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
    ) -> std::result::Result<Receiver<(TaskMessage, Delivery)>, Self::Error>;
}
