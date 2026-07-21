//! Audit configuration — loaded from `.grok/config.toml`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Audit subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Whether audit logging is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Path to the SQLite audit database.
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    /// Retention: maximum age of entries in days.
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Retention: maximum database size in MB.
    #[serde(default = "default_retention_max_mb")]
    pub retention_max_mb: u64,
    /// Whether cryptographic chaining is enabled.
    #[serde(default = "default_true")]
    pub crypto_chain: bool,
    /// Whether secret redaction is enabled.
    #[serde(default = "default_true")]
    pub redact_secrets: bool,
    /// Custom redaction patterns (regex).
    #[serde(default)]
    pub redact_patterns: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".grok/audit/audit.db")
}

fn default_retention_days() -> u32 {
    90
}

fn default_retention_max_mb() -> u64 {
    500
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            db_path: default_db_path(),
            retention_days: default_retention_days(),
            retention_max_mb: default_retention_max_mb(),
            crypto_chain: true,
            redact_secrets: true,
            redact_patterns: vec![],
        }
    }
}
