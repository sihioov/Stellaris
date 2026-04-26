/// dysonsphere/src/mq/rabbit_mq.rs

use crate::error::StellarisError;
use crate::mq::message_queue::MessageQueue;
use crate::message::TaskMessage;
use tokio::sync::mpsc::{channel, Receiver};
use async_trait::async_trait;
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties,
};

use futures_util::stream::StreamExt;

pub struct RabbitMQClient {
    channel: Channel
}

impl RabbitMQClient {
    pub async fn new(uri: &str) -> Result<Self, StellarisError> {
        let conn = Connection::connect(uri, ConnectionProperties::default()).await?;
        let channel = conn.create_channel().await?;

        Ok(Self {channel})
    }
}

#[async_trait]
impl MessageQueue for RabbitMQClient {
    type Error = StellarisError;

    async fn publish(&self, topic: &str, message: TaskMessage) -> Result<(), Self::Error> {
        let payload = serde_json::to_vec(&message)?;
        self.channel.basic_publish(
            "",
            topic,
            BasicPublishOptions::default(),
            payload.as_slice(),
            BasicProperties::default(),
        ).await?.await?;

        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<Receiver<TaskMessage>, Self::Error> {
        // Create MPSC channel
        let (tx, rx) = channel(100);

        // Declare queue as topic
        let queue = self.channel.queue_declare(
                topic,
                QueueDeclareOptions::default(),
                FieldTable::default()).await?;

        // Create consumer for subscribing queue
        let mut consumer = self.channel.basic_consume(
            queue.name().as_str(),
            "",
            BasicConsumeOptions { no_ack: true, ..Default::default() },
            FieldTable::default(),
        ).await?;

        // 백그라운드 테스크로 delivery + TaskMessage 변환 -> 채널 전송
        tokio::spawn(async move {
            while let Some(Ok(delivery)) = consumer.next().await {
                // lapin::message::Delivery가 바로 오기 때문에
                // 더 이상 튜플이 아니라 delivery.data 를 사용합니다.
                if let Ok(task) = serde_json::from_slice::<TaskMessage>(&delivery.data) {
                    let _ = tx.send(task).await;
                }
            }
        });

        Ok(rx)
    }
}