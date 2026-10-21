use serde::ser::{Serialize, SerializeStruct, Serializer};

/// Every fallible command returns this error. It serialises to the small `{code, message, detail}`
/// shape the frontend understands, so the UI can distinguish policy blocks from transient errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Policy(String),
    #[error("{0}")]
    RateLimited(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Malformed(String),
    #[error("{0}")]
    Cancelled(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Db(String),
    #[error("{0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Network(_) => "network",
            AppError::Unavailable(_) => "unavailable",
            AppError::Policy(_) => "policy",
            AppError::RateLimited(_) => "rate_limited",
            AppError::Timeout(_) => "timeout",
            AppError::Malformed(_) => "malformed",
            AppError::Cancelled(_) => "cancelled",
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::Internal(_) => "internal",
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        AppError::Validation(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        AppError::Internal(message.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        AppError::Io(format!("filesystem: {value}"))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        AppError::Db(format!("database: {value}"))
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::Malformed(format!("json: {value}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        AppError::Network(format!("http: {value}"))
    }
}

impl From<url::ParseError> for AppError {
    fn from(value: url::ParseError) -> Self {
        AppError::Validation(format!("invalid URL: {value}"))
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}
