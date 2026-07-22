#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP error: {status} {message}")]
    Http { status: u16, message: String },

    #[error("Request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Not authenticated. Call authenticate() first.")]
    NotAuthenticated,

    #[error("{0}")]
    Other(String),
}

impl ApiError {
    pub fn status_code(&self) -> Option<u16> {
        match self {
            ApiError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}
