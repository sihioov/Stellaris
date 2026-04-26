use std::{path::PathBuf, sync::Arc, time::Duration};
use dysonsphere::db::task_table_file::FileTaskTable;
use dysonsphere::mq::rabbit_mq::RabbitMQClient;
use dysonsphere::mq::message_queue::MessageQueue;
use laniakea::worker;

#[tokio::main]
async fn main() -> dysonsphere::error::Result<()> {
    env_logger::init();

    let source = std::env::var("LANIAKEA_SOURCE").unwrap_or_else(|_| "file".into());

    match source.as_str() {
        "file" => {
            let path = std::env::var("LANIAKEA_FILE_PATH")
                .unwrap_or_else(|_| "tasks.json".into());
            log::info!("Starting file mode (path={path})");
            let table = Arc::new(FileTaskTable::new(PathBuf::from(path)));
            worker::run_file_loop(table, Duration::from_secs(1)).await
        }
        "rabbitmq" => {
            let uri = std::env::var("LANIAKEA_RABBITMQ_URI")
                .unwrap_or_else(|_| "amqp://127.0.0.1:5672/%2f".into());
            let topic = std::env::var("LANIAKEA_RABBITMQ_TOPIC")
                .unwrap_or_else(|_| "tasks".into());
            log::info!("Starting rabbitmq mode (uri={uri}, topic={topic})");
            log::warn!("RabbitMQ mode: no_ack=true (at-most-once). Status not persisted. MVP only.");
            let client = RabbitMQClient::new(&uri).await?;
            let rx = client.subscribe(&topic).await?;
            worker::run_rabbit_loop(rx).await
        }
        other => {
            log::warn!("Unknown LANIAKEA_SOURCE={other}, falling back to file");
            let table = Arc::new(FileTaskTable::new(PathBuf::from("tasks.json")));
            worker::run_file_loop(table, Duration::from_secs(1)).await
        }
    }
}
