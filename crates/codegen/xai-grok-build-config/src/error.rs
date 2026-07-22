/// Errors from config loading / parsing.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(String),
    #[error("Serialization error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e.to_string())
    }
}
