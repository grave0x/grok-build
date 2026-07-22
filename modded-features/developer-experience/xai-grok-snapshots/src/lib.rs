//! Session Snapshots & Restore.
//!
//! Save and restore agent conversation state as versioned snapshots.
//! Supports creating, listing, restoring, and comparing snapshots.
//!
//! Spec: modded-featureSpecs/developer-experience/05-session-snapshots.md

/// A saved session snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSnapshot {
    pub id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub description: String,
    pub tool_call_count: u64,
    pub message_count: u64,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
}

/// Snapshot format version for forward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotVersion {
    V1,
}

impl Default for SnapshotVersion {
    fn default() -> Self { Self::V1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_serialization() {
        let snap = SessionSnapshot {
            id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            created_at: chrono::Utc::now(),
            description: "Test snapshot".into(),
            tool_call_count: 10,
            message_count: 25,
            token_usage: TokenUsage { input: 5000, output: 2000 },
        };
        let json = serde_json::to_string(&snap).unwrap();
        let parsed: SessionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.description, "Test snapshot");
    }
}
