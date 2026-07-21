//! Tamper-evident audit trail engine.
//!
//! Records every agent action — tool invocations, file edits, bash commands,
//! network requests, and model interactions — in an append-only SQLite journal
//! with optional cryptographic chaining via BLAKE3. Provides `grok audit`
//! subcommands for export, search, and tailing.
//!
//! # Architecture
//!
//! ```text
//! Agent Loop → Hook Events → AuditSubscriber → AuditStore → SQLite journal
//!                                                ↑
//!                                        CryptoChainer (BLAKE3)
//! ```
//!
//! # Cryptographic Chaining
//!
//! Each entry carries a `chain_hash = BLAKE3(prev_chain_hash || entry_json)`.
//! Entry N is cryptographically bound to entry N-1, making retroactive
//! modification detectable via `grok audit verify`.

pub mod chainer;
pub mod config;
pub mod entry;
pub mod export;
pub mod journal;
pub mod redact;
pub mod store;

use crate::chainer::CryptoChainer;
use crate::entry::{AuditEntry, AuditEventType};
use crate::store::AuditStore;
use std::sync::Arc;
use tokio::sync::Mutex;

/// The main audit engine handle — shared across the agent loop.
pub struct AuditEngine {
    store: Arc<Mutex<AuditStore>>,
    chainer: Option<CryptoChainer>,
    redactor: redact::SecretRedactor,
}

impl AuditEngine {
    /// Open or create the audit database at `db_path`.
    pub async fn open(
        db_path: impl AsRef<std::path::Path>,
        chaining_enabled: bool,
    ) -> anyhow::Result<Self> {
        let store = Arc::new(Mutex::new(AuditStore::open(db_path).await?));
        let chainer = if chaining_enabled {
            Some(CryptoChainer::new())
        } else {
            None
        };
        Ok(Self {
            store,
            chainer,
            redactor: redact::SecretRedactor::default(),
        })
    }

    /// Append a new audit entry. If chaining is enabled, the entry's
    /// `chain_hash` links to the previous entry.
    pub async fn append(&self, mut entry: AuditEntry) -> anyhow::Result<i64> {
        // Redact secrets from params before storing.
        if let Some(ref params) = entry.params {
            entry.params = Some(self.redactor.redact_json(params.clone()));
        }

        // Compute chain hash if chaining is enabled.
        if let Some(ref chainer) = self.chainer {
            entry.chain_hash = chainer.compute(
                entry.prev_chain_hash.as_deref(),
                &entry,
            );
        }

        let mut store = self.store.lock().await;
        store.insert(&entry).await
    }

    /// Query entries for a session.
    pub async fn query_by_session(
        &self,
        session_id: &uuid::Uuid,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        let store = self.store.lock().await;
        store.query_by_session(session_id).await
    }

    /// Query entries by event type within a time range.
    pub async fn query_by_event_type(
        &self,
        event_type: AuditEventType,
        since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        let store = self.store.lock().await;
        store.query_by_event_type(event_type, since).await
    }

    /// Search entries by full-text query against params and result summaries.
    pub async fn search(&self, query: &str) -> anyhow::Result<Vec<AuditEntry>> {
        let store = self.store.lock().await;
        store.search(query).await
    }

    /// Verify the cryptographic chain integrity.
    /// Returns (index, expected, actual) of the first broken entry, or None if intact.
    pub async fn verify_chain(&self) -> anyhow::Result<Option<(usize, String, String)>> {
        let store = self.store.lock().await;
        store.verify_chain().await
    }

    /// Get store statistics.
    pub async fn status(&self) -> anyhow::Result<StoreStatus> {
        let store = self.store.lock().await;
        store.status().await
    }
}

/// Store-level status information.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoreStatus {
    pub entry_count: u64,
    pub db_size_bytes: u64,
    pub oldest_entry: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_entry: Option<chrono::DateTime<chrono::Utc>>,
    pub chain_intact: Option<bool>,
}
