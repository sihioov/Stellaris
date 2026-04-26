//! task/mod.rs

pub mod dispatcher;

pub use dispatcher::{RabbitMQTaskDispatcher, TaskDispatcher, WorkerInfo, WorkerStatus};
