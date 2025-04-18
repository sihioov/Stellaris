use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Processed,
    Failed
}