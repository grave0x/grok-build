//! BLAKE3 cryptographic chaining for tamper-evident audit logs.

use blake3::Hasher;
use crate::entry::AuditEntry;

/// Computes chain hashes that link audit entries in a tamper-evident sequence.
///
/// ```text
/// Entry N chain_hash = BLAKE3(prev_chain_hash || entry_payload)
/// ```
///
/// If entry N is modified, its chain_hash won't match the recomputed value,
/// AND entry N+1's prev_chain_hash won't match either — creating a chain break.
pub struct CryptoChainer;

impl CryptoChainer {
    pub fn new() -> Self {
        Self
    }

    /// Compute the chain hash for a new entry.
    ///
    /// `prev_chain_hash` is None for the first entry in a chain;
    /// in that case we hash a sentinel value `"genesis"` as the previous link.
    pub fn compute(
        &self,
        prev_chain_hash: Option<&str>,
        entry: &AuditEntry,
    ) -> String {
        let mut hasher = Hasher::new();

        // Feed the previous link (or genesis sentinel).
        match prev_chain_hash {
            Some(h) => _ = hasher.update(h.as_bytes()),
            None => _ = hasher.update(b"genesis"),
        }

        // Feed the entry payload (excludes id and chain_hash itself).
        hasher.update(b"||");
        hasher.update(entry.hash_payload().as_bytes());

        let hash = hasher.finalize();
        hash.to_hex().to_string()
    }

    /// Verify a sequence of entries. Returns (index, expected, actual) for
    /// the first broken entry, or None if the chain is intact.
    pub fn verify_chain(entries: &[AuditEntry]) -> Option<(usize, String, String)> {
        let chainer = Self::new();

        for (i, entry) in entries.iter().enumerate() {
            let prev = if i == 0 {
                None
            } else {
                Some(entries[i - 1].chain_hash.as_str())
            };

            let expected = chainer.compute(prev, entry);
            if expected != entry.chain_hash {
                return Some((i, expected, entry.chain_hash.clone()));
            }
        }

        None
    }
}

impl Default for CryptoChainer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{AuditEntry, AuditEventType};
    use uuid::Uuid;

    fn make_entry(session: Uuid, seq: &str) -> AuditEntry {
        AuditEntry {
            id: None,
            session_id: session,
            timestamp: chrono::Utc::now(),
            event_type: AuditEventType::PostToolUse,
            tool_name: Some("Bash".into()),
            params: Some(serde_json::json!({"command": seq})),
            result_summary: Some(seq.into()),
            duration_ms: Some(100),
            exit_code: Some(0),
            model: None,
            prompt_hash: None,
            response_hash: None,
            token_count_input: None,
            token_count_output: None,
            cwd: "/".into(),
            chain_hash: String::new(), // to be filled
            prev_chain_hash: None,
        }
    }

    #[test]
    fn test_chain_integrity() {
        let chainer = CryptoChainer::new();
        let session = Uuid::new_v4();
        let mut entries: Vec<AuditEntry> = vec![];

        for i in 0..5 {
            let mut entry = make_entry(session, &format!("seq-{i}"));
            let prev = entries.last().map(|e: &AuditEntry| e.chain_hash.as_str());
            entry.prev_chain_hash = prev.map(|s| s.to_string());
            entry.chain_hash = chainer.compute(prev, &entry);
            entries.push(entry);
        }

        assert!(CryptoChainer::verify_chain(&entries).is_none());
    }

    #[test]
    fn test_chain_break_detection() {
        let chainer = CryptoChainer::new();
        let session = Uuid::new_v4();
        let mut entries: Vec<AuditEntry> = vec![];

        for i in 0..5 {
            let mut entry = make_entry(session, &format!("seq-{i}"));
            let prev = entries.last().map(|e| e.chain_hash.as_str());
            entry.prev_chain_hash = prev.map(|s| s.to_string());
            entry.chain_hash = chainer.compute(prev, &entry);
            entries.push(entry);
        }

        // Tamper with entry 2.
        entries[2].result_summary = Some("tampered!".into());

        let broken = CryptoChainer::verify_chain(&entries);
        assert!(broken.is_some());
        assert_eq!(broken.unwrap().0, 2);
    }
}
