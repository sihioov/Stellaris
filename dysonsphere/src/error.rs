use std::fmt;
#[derive(Debug)]
pub enum StellarisError {
    DatabaseError(String),
    NotFound(String),
    IoError(String),
    SerdeError(String),
    DefaultError,
    MQError(String),
    JsonError(String),
}

impl fmt::Display for StellarisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StellarisError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            StellarisError::NotFound(msg) => write!(f, "Not found: {}", msg),
            StellarisError::IoError(msg) => write!(f, "I/O error: {}", msg),
            StellarisError::SerdeError(msg) => write!(f, "Serialization error: {}", msg),
            StellarisError::DefaultError => write!(f, "Default error"),
            StellarisError::MQError(msg) => write!(f, "MQ error: {}", msg),
            StellarisError::JsonError(msg) => write!(f, "JSON error: {}", msg),
        }
    }
}

impl std::error::Error for StellarisError {}

/// Change to SetllarisError
impl From<std::io::Error> for StellarisError {
    fn from(err: std::io::Error) -> Self {
        StellarisError::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for StellarisError {
    fn from(err: serde_json::Error) -> Self {
        StellarisError::SerdeError(err.to_string())
    }
}

impl From<lapin::Error> for StellarisError {
    fn from(error: lapin::Error) -> Self {
        StellarisError::MQError(format!("RabbitMQ error: {}", error))
    }
}

/// Result alias
pub type Result<T> = std::result::Result<T, StellarisError>;
