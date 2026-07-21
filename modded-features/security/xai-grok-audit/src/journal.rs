//! SQLite journal operations for the audit store.

use crate::entry::{AuditEntry, AuditEventType};
use rusqlite::{params, Connection};
use std::path::Path;
use std::str::FromStr;
use xai_sqlite_journal::JournalMode;

/// Wrapper around a SQLite connection for audit entries.
pub struct AuditJournal {
    conn: Connection,
    db_path: std::path::PathBuf,
}

impl AuditJournal {
    /// Open (or create) the audit database with the correct journal mode.
    pub fn open(db_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();

        // Use the filesystem-aware journal mode selector.
        let mode = JournalMode::for_db_path(&db_path);
        let effective_path = mode.effective_db_path(&db_path);

        let conn = Connection::open(&effective_path)?;

        let journal_pragma = match mode {
            JournalMode::Wal => "wal",
            JournalMode::Truncate => "truncate",
        };
        conn.pragma_update(None, "journal_mode", journal_pragma)?;
        conn.pragma_update(None, "busy_timeout", 5000)?;

        Ok(Self { conn, db_path })
    }

    /// Create the audit_log table and indices if they don't exist.
    pub fn ensure_schema(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id      TEXT NOT NULL,
                timestamp       TEXT NOT NULL,
                event_type      TEXT NOT NULL,
                tool_name       TEXT,
                params          TEXT,
                result_summary  TEXT,
                duration_ms     INTEGER,
                exit_code       INTEGER,
                model           TEXT,
                prompt_hash     TEXT,
                response_hash   TEXT,
                token_count_input   INTEGER,
                token_count_output  INTEGER,
                cwd             TEXT NOT NULL,
                chain_hash      TEXT NOT NULL,
                prev_chain_hash TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_audit_session_time
                ON audit_log(session_id, timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_event_time
                ON audit_log(event_type, timestamp);
            CREATE INDEX IF NOT EXISTS idx_audit_timestamp
                ON audit_log(timestamp);",
        )?;
        Ok(())
    }

    /// Insert a single audit entry. Returns the row ID.
    pub fn insert(&self, entry: &AuditEntry) -> anyhow::Result<i64> {
        let params_json = entry
            .params
            .as_ref()
            .map(|v| v.to_string());

        self.conn.execute(
            "INSERT INTO audit_log
                (session_id, timestamp, event_type, tool_name, params,
                 result_summary, duration_ms, exit_code, model,
                 prompt_hash, response_hash, token_count_input, token_count_output,
                 cwd, chain_hash, prev_chain_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                entry.session_id.to_string(),
                entry.timestamp.to_rfc3339(),
                entry.event_type.to_string(),
                entry.tool_name,
                params_json,
                entry.result_summary,
                entry.duration_ms,
                entry.exit_code,
                entry.model,
                entry.prompt_hash,
                entry.response_hash,
                entry.token_count_input,
                entry.token_count_output,
                entry.cwd,
                entry.chain_hash,
                entry.prev_chain_hash,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Query entries by session ID.
    pub fn query_by_session(
        &self,
        session_id: &uuid::Uuid,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, timestamp, event_type, tool_name, params,
                    result_summary, duration_ms, exit_code, model,
                    prompt_hash, response_hash, token_count_input, token_count_output,
                    cwd, chain_hash, prev_chain_hash
             FROM audit_log
             WHERE session_id = ?1
             ORDER BY timestamp ASC",
        )?;

        let entries = stmt.query_map(params![session_id.to_string()], |row| {
            Self::row_to_entry(row)
        })?;

        Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Query entries by event type, optionally since a timestamp.
    pub fn query_by_event_type(
        &self,
        event_type: &str,
        since: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        if let Some(since) = since {
            let mut stmt = self.conn.prepare(
                "SELECT id, session_id, timestamp, event_type, tool_name, params,
                        result_summary, duration_ms, exit_code, model,
                        prompt_hash, response_hash, token_count_input, token_count_output,
                        cwd, chain_hash, prev_chain_hash
                 FROM audit_log
                 WHERE event_type = ?1 AND timestamp >= ?2
                 ORDER BY timestamp ASC",
            )?;
            let entries = stmt.query_map(
                params![event_type, since.to_rfc3339()],
                |row| Self::row_to_entry(row),
            )?;
            Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, session_id, timestamp, event_type, tool_name, params,
                        result_summary, duration_ms, exit_code, model,
                        prompt_hash, response_hash, token_count_input, token_count_output,
                        cwd, chain_hash, prev_chain_hash
                 FROM audit_log
                 WHERE event_type = ?1
                 ORDER BY timestamp ASC",
            )?;
            let entries = stmt.query_map(params![event_type], |row| Self::row_to_entry(row))?;
            Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
        }
    }

    /// Full-text search across params and result_summary.
    pub fn search(&self, query: &str) -> anyhow::Result<Vec<AuditEntry>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, timestamp, event_type, tool_name, params,
                    result_summary, duration_ms, exit_code, model,
                    prompt_hash, response_hash, token_count_input, token_count_output,
                    cwd, chain_hash, prev_chain_hash
             FROM audit_log
             WHERE params LIKE ?1 OR result_summary LIKE ?1
             ORDER BY timestamp ASC",
        )?;
        let entries = stmt.query_map(params![pattern], |row| Self::row_to_entry(row))?;
        Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Get all entries ordered by timestamp.
    pub fn all_entries(&self) -> anyhow::Result<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, timestamp, event_type, tool_name, params,
                    result_summary, duration_ms, exit_code, model,
                    prompt_hash, response_hash, token_count_input, token_count_output,
                    cwd, chain_hash, prev_chain_hash
             FROM audit_log
             ORDER BY timestamp ASC",
        )?;
        let entries = stmt.query_map([], |row| Self::row_to_entry(row))?;
        Ok(entries.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Count total entries.
    pub fn count(&self) -> anyhow::Result<usize> {
        Ok(
            self.conn
                .query_row("SELECT COUNT(*) FROM audit_log", [], |row| {
                    row.get::<_, usize>(0)
                })?,
        )
    }

    /// Get database file size in bytes.
    pub fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(std::fs::metadata(&self.db_path)?.len())
    }

    /// Get the oldest entry timestamp.
    pub fn oldest_timestamp(
        &self,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        let result: Option<String> = self.conn.query_row(
            "SELECT timestamp FROM audit_log ORDER BY timestamp ASC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        result
            .map(|s| chrono::DateTime::from_str(&s))
            .transpose()
            .map_err(Into::into)
    }

    /// Get the newest entry timestamp.
    pub fn newest_timestamp(
        &self,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        let result: Option<String> = self.conn.query_row(
            "SELECT timestamp FROM audit_log ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        result
            .map(|s| chrono::DateTime::from_str(&s))
            .transpose()
            .map_err(Into::into)
    }

    /// Prune entries older than `retention_days`.
    pub fn prune_old(&self, retention_days: u32) -> anyhow::Result<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        Ok(self.conn.execute(
            "DELETE FROM audit_log WHERE timestamp < ?1",
            params![cutoff.to_rfc3339()],
        )?)
    }

    /// Prune oldest entries if db size exceeds `max_mb`.
    pub fn prune_by_size(&self, max_mb: u64) -> anyhow::Result<usize> {
        let current_size = self.db_size_bytes()?;
        let max_bytes = max_mb * 1024 * 1024;
        if current_size <= max_bytes {
            return Ok(0);
        }
        // Delete ~20% of oldest entries when over the limit.
        let total: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| {
                row.get(0)
            })?;
        let to_delete = total / 5;
        Ok(self.conn.execute(
            "DELETE FROM audit_log WHERE id IN (
                SELECT id FROM audit_log ORDER BY timestamp ASC LIMIT ?1
            )",
            params![to_delete],
        )?)
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEntry> {
        let params_str: Option<String> = row.get(5)?;
        let params = params_str
            .and_then(|s| serde_json::from_str(&s).ok());

        let timestamp_str: String = row.get(2)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
            .map(|dt| dt.to_utc())
            .unwrap_or_default();

        let session_id_str: String = row.get(1)?;
        let session_id = uuid::Uuid::parse_str(&session_id_str).unwrap_or_default();

        let event_type_str: String = row.get(3)?;
        let event_type = event_type_str.parse().unwrap_or(AuditEventType::PostToolUse);

        Ok(AuditEntry {
            id: row.get(0)?,
            session_id,
            timestamp,
            event_type,
            tool_name: row.get(4)?,
            params,
            result_summary: row.get(6)?,
            duration_ms: row.get(7)?,
            exit_code: row.get(8)?,
            model: row.get(9)?,
            prompt_hash: row.get(10)?,
            response_hash: row.get(11)?,
            token_count_input: row.get(12)?,
            token_count_output: row.get(13)?,
            cwd: row.get(14)?,
            chain_hash: row.get(15)?,
            prev_chain_hash: row.get(16)?,
        })
    }
}
