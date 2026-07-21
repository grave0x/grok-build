//! Audit entry types and event classification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The core audit record — one row per agent action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Auto-increment primary key (None before insert).
    pub id: Option<i64>,
    /// Session identifier from SessionStart.
    pub session_id: Uuid,
    /// UTC timestamp when the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Categorizes the event.
    pub event_type: AuditEventType,
    /// Tool name for tool invocations ("Bash", "Read", "Edit", etc.).
    pub tool_name: Option<String>,
    /// Params passed to the tool (secrets redacted before storage).
    pub params: Option<serde_json::Value>,
    /// Truncated result summary (first 1KB).
    pub result_summary: Option<String>,
    /// Duration of the tool call in milliseconds.
    pub duration_ms: Option<u64>,
    /// Exit code for bash/edit/run calls.
    pub exit_code: Option<i32>,
    /// Model name for model interactions.
    pub model: Option<String>,
    /// BLAKE3 hash of the full prompt.
    pub prompt_hash: Option<String>,
    /// BLAKE3 hash of the full response.
    pub response_hash: Option<String>,
    /// Input token count.
    pub token_count_input: Option<u64>,
    /// Output token count.
    pub token_count_output: Option<u64>,
    /// Working directory at time of call.
    pub cwd: String,
    /// BLAKE3(prev_chain_hash || serialized_entry) for tamper detection.
    pub chain_hash: String,
    /// Previous entry's chain_hash; None for first entry.
    pub prev_chain_hash: Option<String>,
}

/// Event type taxonomy for audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString)]
#[strum(serialize_all = "PascalCase")]
pub enum AuditEventType {
    SessionStart,
    SessionEnd,
    PreToolUse,
    PostToolUse,
    ModelPrompt,
    ModelResponse,
    FileWrite,
    FileRead,
    BashCommand,
    NetworkRequest,
    SandboxViolation,
    PermissionDenied,
}

impl AuditEntry {
    /// Create a new entry with required fields filled; defaults for optional.
    pub fn new(
        session_id: Uuid,
        event_type: AuditEventType,
        cwd: String,
        chain_hash: String,
        prev_chain_hash: Option<String>,
    ) -> Self {
        Self {
            id: None,
            session_id,
            timestamp: Utc::now(),
            event_type,
            tool_name: None,
            params: None,
            result_summary: None,
            duration_ms: None,
            exit_code: None,
            model: None,
            prompt_hash: None,
            response_hash: None,
            token_count_input: None,
            token_count_output: None,
            cwd,
            chain_hash,
            prev_chain_hash,
        }
    }

    /// Build the serialization payload for chain hash computation.
    /// Excludes `id` and `chain_hash` itself (circularity).
    pub fn hash_payload(&self) -> String {
        format!(
            "{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
            self.session_id,
            self.timestamp.to_rfc3339(),
            self.event_type,
            self.tool_name,
            self.params.as_ref().map(|v| v.to_string()),
            self.result_summary,
            self.duration_ms,
            self.exit_code,
            self.model,
            self.prompt_hash,
            self.response_hash,
            self.token_count_input,
            self.token_count_output,
            self.cwd,
        )
    }
}
