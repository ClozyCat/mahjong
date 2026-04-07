use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum EngineError {
    InvalidState(String),
    LegacyDecode(String),
    Serde(serde_json::Error),
}

impl EngineError {
    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::InvalidState(message.into())
    }

    pub fn legacy_decode(message: impl Into<String>) -> Self {
        Self::LegacyDecode(message.into())
    }
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState(message) => write!(f, "invalid_state: {message}"),
            Self::LegacyDecode(message) => write!(f, "legacy_decode: {message}"),
            Self::Serde(error) => write!(f, "serde_error: {error}"),
        }
    }
}

impl Error for EngineError {}

impl From<serde_json::Error> for EngineError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value)
    }
}
