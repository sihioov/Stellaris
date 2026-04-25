use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanopusError {
    InvalidInput(String),
    InvalidTransition(String),
    Io(String),
    Backend(String),
    Tool(String),
    Runtime(String),
}

impl fmt::Display for CanopusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CanopusError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            CanopusError::InvalidTransition(message) => write!(f, "invalid transition: {message}"),
            CanopusError::Io(message) => write!(f, "io error: {message}"),
            CanopusError::Backend(message) => write!(f, "backend error: {message}"),
            CanopusError::Tool(message) => write!(f, "tool error: {message}"),
            CanopusError::Runtime(message) => write!(f, "runtime error: {message}"),
        }
    }
}

impl std::error::Error for CanopusError {}

impl From<std::io::Error> for CanopusError {
    fn from(value: std::io::Error) -> Self {
        CanopusError::Io(value.to_string())
    }
}

pub type CanopusResult<T> = Result<T, CanopusError>;
