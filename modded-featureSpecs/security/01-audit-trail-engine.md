# Spec 01 — Audit Trail Engine

- **Priority:** P0
- **Crate:** `xai-grok-audit` (new)
- **Depends on:** `xai-grok-hooks`, `xai-sqlite-journal`, `xai-grok-config-types`
- **Status:** Draft

---

## Overview

A structured, tamper-evident audit log that records every agent action — tool invocations, file edits, bash commands, network requests, and model interactions — in an append-only SQLite journal with optional cryptographic chaining. Provides `grok audit` subcommands for export, search, and tailing.

## Motivation

Enterprise compliance (SOC2, FedRAMP, SOX) requires tamper-proof audit trails of all actions an automated agent takes. The current hook system is stateless and script-based — scripts fire and forget, with no built-in persistence, structured schema, or tamper detection. Security teams cannot today answer "what did the agent do, when, and with what result?" for a given session.

The gravemod hardening plan (10 issues) addresses *prevention* (sandbox, permissions, SSRF). This addresses *detection and accountability* — the second pillar of defense in depth.

## Goals

- Record every tool call (name, params, result, duration, exit status) to a durable local store.
- Record every model interaction (prompt, response, token counts, model name).
- Produce an export format suitable for ingestion by SIEM systems (JSON, CEF, or LEEF).
- Support cryptographic chaining so that entries cannot be retroactively modified without detection.
- Introduce `grok audit` CLI with `export`, `tail`, `search`, `status` subcommands.
- Minimal performance overhead (<5ms median per log write).
- Configurable retention (by age, by entry count, by disk usage).

## Non-Goals

- Real-time streaming to an external SIEM agent (out of scope; the export format enables downstream ingestion).
- Cross-machine correlation (single-host per audit store; future work could centralize via MCP).
- Replacing the hook system: audit is a *consumer* of hook events, not a *replacement*.

## Design

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Agent Loop                         │
│  (xai-grok-shell / xai-grok-agent)                  │
└──────────┬──────────────────────────────────────────┘
           │ tool_invoked / model_interaction events
           ▼
┌──────────────────────┐     ┌──────────────────────────┐
│   xai-grok-hooks      │────▶│   xai-grok-audit         │
│   (PreToolUse,        │     │   (new crate)            │
│    PostToolUse,       │     │                          │
│    SessionStart/End)  │     │  ┌──────────────────┐    │
└──────────────────────┘     │  │ Crypto Chainer    │    │
                             │  │ (BLAKE3 hash link)│    │
                             │  └──────────────────┘    │
                             │  ┌──────────────────┐    │
                             │  │ SQLite Journal    │    │
                             │  │ (xai-sqlite-      │    │
                             │  │  journal wrapper) │    │
                             │  └──────────────────┘    │
                             └──────────────────────────┘
```

### Data Model

```rust
/// The core audit record.
struct AuditEntry {
    id: i64,                    // auto-increment primary key
    session_id: Uuid,           // from SessionStart
    timestamp: DateTime<Utc>,   // when the event occurred
    event_type: AuditEventType, // enum below
    tool_name: Option<String>,  // "Bash", "Read", "SearchReplace", ...
    params: Option<JsonValue>,  // redacted params (secrets stripped)
    result_summary: Option<String>,  // truncated result (first 1KB)
    duration_ms: Option<u64>,   // how long the tool call took
    exit_code: Option<i32>,     // for bash, edit, etc.
    model: Option<String>,      // for model interactions
    prompt_hash: Option<String>,// BLAKE3 hash of the full prompt
    response_hash: Option<String>,// BLAKE3 hash of the full response
    token_count_input: Option<u64>,
    token_count_output: Option<u64>,
    cwd: String,                // working directory at time of call
    chain_hash: String,         // BLAKE3(prev_chain_hash || entry_json)
    prev_chain_hash: Option<String>, // null for first entry
}

enum AuditEventType {
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
```

### Storage

- Default location: `~/.grok/audit/audit.db` (configurable via `GROK_AUDIT_DB` env var)
- SQLite with WAL mode for concurrent reads during writes
- Auto-rotate: when size exceeds configurable threshold (default 100MB), rotate to `audit.db.1`, `audit.db.2`, etc.
- Retention: configurable by age (`retention_days = 90`), count (`retention_entries = 1_000_000`), or size (`retention_max_mb = 500`)
- Table: `audit_log` with an index on `(session_id, timestamp)` and `(event_type, timestamp)`

### Cryptographic Chaining

```
Entry N:
  chain_hash = BLAKE3(
    prev_chain_hash ||                         // Entry N-1's chain_hash
    entry_id || session_id || timestamp ||      // metadata
    event_type || tool_name ||                  // event details
    params_hash || result_hash                  // BLAKE3 of the actual data
  )
```

Chain break detection: `grok audit verify` recomputes all chains and reports the first entry where `chain_hash` doesn't match. A break indicates tampering (or corruption).

### CLI Interface

```
grok audit export --format json|cef|csv [--since <datetime>] [--session <uuid>]
grok audit tail [-n 50] [--follow] [--event-type Bash]
grok audit search "<query>" [--since <datetime>] [--event-type Edit]
grok audit status        -- show db size, entry count, chain integrity
grok audit verify        -- verify cryptographic chain integrity
grok audit prune         -- manually trigger retention cleanup
```

### Config (`.grok/config.toml`)

```toml
[audit]
enabled = true
db_path = "~/.grok/audit/audit.db"  # default
retention_days = 90
retention_max_mb = 500
crypto_chain = true                  # enable cryptographic linking
redact_secrets = true                # strip API keys, passwords from params
redact_patterns = ["sk-*", "xai-*"] # custom redaction patterns
```

### Hook Integration

The audit engine subscribes to hook events internally. It does NOT require users to install hook scripts. Instead, the crate provides an `AuditHookSubscriber` that implements a trait from `xai-grok-hooks`:

```rust
impl HookSubscriber for AuditSubscriber {
    fn on_event(&self, event: HookEvent) -> Result<()> {
        let entry = AuditEntry::from_hook_event(event);
        self.journal.append(entry)
    }
}
```

## Implementation Plan

### Phase 1 — MVP (2-3 weeks)
1. Create `xai-grok-audit` crate with SQLite schema, `AuditEntry`, `AuditStore`.
2. Implement basic append + query (by session, by event_type, by time range).
3. Add `grok audit export --format json` and `grok audit tail`.
4. Wire into hook system via `HookSubscriber`.
5. Add secret redaction (regex-based) for params.
6. Tests: round-trip append/query, redaction, concurrent writers.

### Phase 2 — Polish (1-2 weeks)
1. Implement cryptographic chaining and `grok audit verify`.
2. Add `grok audit search` with full-text search on JSON params.
3. Auto-rotation and retention policy enforcement.
4. `grok audit status` with chain integrity indicator.
5. CEF and CSV export formats.
6. Integration test: audit entries match actual tool calls across a full session.

### Phase 3 — Advanced (2-3 weeks)
1. Optional GPG-signing of chain roots for external audit verification.
2. Streaming export to S3/GCS via configurable sink.
3. Audit viewer TUI panel (timeline mode) — tie into spec #6.
4. Performance benchmark: target <5ms per log write at 10K entries.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Append, query, redact, chain computation, retention trigger |
| Integration | Full session replay → check every tool call has a matching audit entry |
| Tamper | Manually modify SQLite → `audit verify` detects break |
| Performance | 100K entries, measure write latency, query latency, db size |
| Concurrency | 10 tokio tasks writing simultaneously → no corruption |

## Open Questions

1. Should we support remote audit sinks (syslog, Fluentd) at MVP or Phase 2?
2. Chain hash: store in SQLite row or separate chain file (append-only log)?
   - *Decision:* Store in SQLite row for simplicity. The `chain_hash` column IS the chain.
3. How to handle very large params/results (>1MB)? Truncate with a hash of the full payload.
4. Should `grok audit` be opt-in or opt-out for gravemod? *Decision:* Opt-in for MVP, default-enabled in Phase 2.
