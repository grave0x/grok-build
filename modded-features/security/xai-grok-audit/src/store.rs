//! SQLite-backed persistent audit store.

use crate::chainer::CryptoChainer;
use crate::entry::{AuditEntry, AuditEventType};
use crate::journal::AuditJournal;
use crate::StoreStatus;
use std::path::Path;

/// Persistent audit store wrapping a SQLite journal.
pub struct AuditStore {
    journal: AuditJournal,
}

impl AuditStore {
    /// Open (or create) the audit database at `db_path`.
    pub async fn open(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let parent = db_path.as_ref().parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await?;

        let journal = AuditJournal::open(db_path)?;
        journal.ensure_schema()?;

        Ok(Self { journal })
    }

    /// Insert a single audit entry. Returns the row ID.
    pub async fn insert(&mut self, entry: &AuditEntry) -> anyhow::Result<i64> {
        self.journal.insert(entry)
    }

    /// Query entries by session ID, ordered by timestamp.
    pub async fn query_by_session(
        &self,
        session_id: &uuid::Uuid,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        self.journal.query_by_session(session_id)
    }

    /// Query entries by event type, optionally filtered by a `since` timestamp.
    pub async fn query_by_event_type(
        &self,
        event_type: AuditEventType,
        since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        let event_type_str = event_type.to_string();
        self.journal.query_by_event_type(&event_type_str, since)
    }

    /// Full-text search across params and result summaries.
    pub async fn search(&self, query: &str) -> anyhow::Result<Vec<AuditEntry>> {
        self.journal.search(query)
    }

    /// Verify the BLAKE3 chain integrity.
    /// Returns (index, expected_hash, actual_hash) of the first broken entry, or None if intact.
    pub async fn verify_chain(&self) -> anyhow::Result<Option<(usize, String, String)>> {
        let entries = self.journal.all_entries()?;
        Ok(CryptoChainer::verify_chain(&entries))
    }

    /// Prune entries older than `retention_days`.
    pub async fn prune_old(&mut self, retention_days: u32) -> anyhow::Result<usize> {
        self.journal.prune_old(retention_days)
    }

    /// Prune if database size exceeds `max_mb`.
    pub async fn prune_by_size(&mut self, max_mb: u64) -> anyhow::Result<usize> {
        self.journal.prune_by_size(max_mb)
    }

    /// Get store status.
    pub async fn status(&self) -> anyhow::Result<StoreStatus> {
        let count = self.journal.count()?;
        let size = self.journal.db_size_bytes()?;
        let oldest = self.journal.oldest_timestamp()?;
        let newest = self.journal.newest_timestamp()?;

        Ok(StoreStatus {
            entry_count: count as u64,
            db_size_bytes: size,
            oldest_entry: oldest,
            newest_entry: newest,
            chain_intact: None, // computed on demand
        })
    }
}
