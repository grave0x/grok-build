# Spec 05 — Session Snapshots & Restore

- **Priority:** P1
- **Crate:** `xai-grok-snapshot` (new)
- **Depends on:** `xai-sqlite-journal`, `xai-grok-shell`, `xai-grok-pager`, `xai-chat-state`
- **Status:** Draft

---

## Overview

Bookmark, serialize, and restore the full state of an agent session — chat history, file states, active goals, scrollback, and MCP server connections. Enables branching experiments, checkpoints before risky operations, and long-running context that survives process restarts.

## Motivation

AI agent sessions accumulate precious context over hundreds of turns. A lost terminal, a crash, or a context-limit restart destroys all that context. Users also want to:
- Save a checkpoint before a risky `search_replace` operation ("save, then try the refactor").
- Branch: try two approaches from the same session state.
- Share a session: serialize a debugging session and send it to a teammate.
- Resume after reboot or after switching machines.

The existing `xai-sqlite-journal` and `xai-chat-state` crates already handle some persistence — this spec unifies and extends them.

## Goals

- `grok snapshot save "message"` — save current session state with a descriptive label.
- `grok snapshot list` — list all snapshots with timestamp, label, message count.
- `grok snapshot load <id>` — restore full session state into a new session.
- `grok snapshot diff <id1> <id2>` — compare two snapshots (message diff, file changes).
- `grok snapshot rm <id>` — delete a snapshot.
- Auto-save on interval (configurable: every N turns, or every M minutes).
- Snapshot includes: chat messages, file modifications (diffs), active goals, scrollback position, MCP server states, environment variables.

## Non-Goals

- Cross-machine snapshot transport (future: `grok snapshot push/pull`).
- Snapshot merge (manual rebase-like operation — future work).
- Cloud backup (SSH/SCP export is sufficient for MVP).

## Design

### Data Model

```rust
struct Snapshot {
    id: Uuid,
    created_at: DateTime<Utc>,
    label: String,                   // user-provided or auto-generated
    turn_count: u32,                 // number of conversation turns
    token_estimate: u64,             // estimated context window usage
    message_count: u32,
    file_diffs: Vec<FileDiff>,       // uncommitted changes at snapshot time
    git_head: Option<String>,        // current git HEAD SHA
    metadata: HashMap<String, String>,// extensible key-value metadata
}

struct FileDiff {
    path: PathBuf,
    old_hash: String,                // BLAKE3 of file at session start (or last snapshot)
    new_hash: String,                // BLAKE3 of file at snapshot time
    diff: String,                    // unified diff
}

struct SnapshotStore {
    conn: SqliteJournal,
    snapshots: BTreeMap<Uuid, SnapshotIndex>,
}

struct SnapshotIndex {
    id: Uuid,
    timestamp: DateTime<Utc>,
    label: String,
    turn_count: u32,
    file_count: u32,
    size_bytes: u64,
}
```

### Storage

- Location: `~/.grok/snapshots/<uuid>/`
- Contents:
  - `snapshot.json` — metadata + index
  - `messages.jsonl` — full chat message log (JSON Lines)
  - `files/` — snapshot of file states (git-like object store, deduplicated by BLAKE3 hash)
  - `state.bin` — binary session state (scrollback position, MCP connections, env vars)
  - `goals.json` — active goals at snapshot time
- Global index: `~/.grok/snapshots/index.db` (SQLite) for fast listing/searching

### CLI Interface

```
grok snapshot save "before risky refactor"    # save with label
grok snapshot save                             # auto-label with timestamp
grok snapshot list                             # list all (short: ls)
grok snapshot list --limit 5 --since 7d        # filtered
grok snapshot show <id>                        # show snapshot details
grok snapshot load <id>                        # restore into new session
grok snapshot diff <id1> <id2>                 # compare two snapshots
grok snapshot rm <id>                          # delete
grok snapshot export <id> --format tar.gz      # export portable archive
grok snapshot import ./session.tar.gz          # import archive
```

### Auto-Save Configuration

```toml
[snapshot]
enabled = true
auto_save_interval_turns = 50     # every N conversation turns
auto_save_interval_minutes = 30   # every M minutes
max_snapshots = 100               # keep last N, auto-prune oldest
prune_on_save = true              # prune when exceeding max_snapshots
```

### TUI Integration

```
┌───────────────────────────────────────┐
│ [Save]        [Load ▾]         [Diff] │
│                                       │
│  Snapshots                            │
│  ┌─────────────────────────────────┐  │
│  │ ID  │ Timestamp   │ Label       │  │
│  │─────│─────────────│─────────────│  │
│  │ #12 │ 10:34:22    │ before-     │  │
│  │     │             │ refactor    │  │
│  │ #11 │ 09:15:01    │ boot-       │  │
│  │     │             │ strapping   │  │
│  └─────────────────────────────────┘  │
└───────────────────────────────────────┘
```

- `grok snapshot save` accessible via slash command `/snapshot-save`.
- Snapshot list in the TUI via modal or side panel.
- Auto-save shows a subtle notification: "Snapshot #12 saved (turn 152)".

## Implementation Plan

### Phase 1 — MVP (2 weeks)
1. Create `xai-grok-snapshot` crate with `Snapshot` data model and `SnapshotStore`.
2. Implement `grok snapshot save` and `grok snapshot list`.
3. Implement snapshot serialization: chat messages, scrollback position, file diffs.
4. Implement `grok snapshot load` — restore into a new session.
5. Tests: save/load round-trip, snapshot list accuracy, file diff capture.

### Phase 2 — Auto-Save & Diff (1-2 weeks)
1. Implement auto-save timers (turn interval + time interval).
2. Implement `grok snapshot diff` (message diff + file diff).
3. Implement auto-prune (keep last N, oldest-first).
4. TUI integration: notification on auto-save, snapshot list side panel.
5. Tests: auto-save fires correctly, pruning respects bounds, diff is accurate.

### Phase 3 — Export/Import (1 week)
1. Implement `grok snapshot export` (tar.gz archive).
2. Implement `grok snapshot import`.
3. Add `grok snapshot show <id>` with detailed metadata.
4. Performance: optimize file deduplication (reuse blobs across snapshots).
5. Tests: export/import round-trip, cross-machine compatibility.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Snapshot serialization/deserialization, file diff computation |
| Integration | Save → restart agent → load → verify chat history and file states match |
| Auto-save | Timer fires at correct interval, doesn't fire when disabled |
| Pruning | 150 snapshots with max=100 → only 100 remain after prune |
| Export/Import | Archive created → extracted on different machine → load succeeds |

## Open Questions

1. Should snapshots include git history beyond HEAD? *Decision:* No — git history is already on disk. Snapshot only stores current HEAD SHA.
2. How to handle MCP server state (connections, subscriptions)? *Decision:* Serialize connection URIs and auth tokens; re-connect on load (idempotent).
3. Large files: should snapshots copy the entire workspace? *Decision:* No — only store diffs from session start. Full file state is on disk already.
