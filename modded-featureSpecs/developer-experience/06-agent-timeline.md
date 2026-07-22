# Spec 06 — Agent State Timeline

- **Priority:** P2
- **Crate:** `xai-grok-timeline` (new, TUI component in `xai-grok-pager`)
- **Depends on:** `xai-grok-pager`, `xai-grok-shell`, `xai-ratatui-inline`
- **Status:** Draft

---

## Overview

A structured, visual timeline panel in the TUI that shows every agent action as a time-ordered entry with duration, status, and result summary. Users can navigate, filter, and jump to any point in the agent's execution history.

## Motivation

The existing scrollback is a linear text log. Finding "what did the agent do 40 turns ago?" requires scrolling up and reading walls of text. A structured timeline with visual affordances (colors, icons, durations, filter controls) makes it dramatically easier to understand, review, and debug agent behavior.

## Goals

- Timeline panel showing every tool call as a card with: tool icon, name, duration, status (running/success/error), truncated result.
- Color-coding by tool type (bash=yellow, edit=green, read=blue, search=purple, error=red).
- Click or keybind to jump scrollback to that entry.
- Filter by tool type, status (errors only), or search text.
- Duration histogram: quick visual of fast vs slow operations.
- Session metrics: total turns, total time, tool mix pie chart.

## Non-Goals

- Replacing the scrollback (timeline is a navigation aid, not a replacement).
- Real-time timeline (updates after each tool call completes, not streaming).
- Sub-turn detail (individual tokens or model reasoning — out of scope).

## Design

### Architecture

```
┌──────────────────────────────────────────────┐
│  xai-grok-pager                               │
│                                               │
│  ┌──────────────────┐  ┌───────────────────┐  │
│  │ Scrollback        │  │ Timeline Panel      │  │
│  │ (main area)       │  │                     │  │
│  │                   │  │ ┌───────────────┐   │  │
│  │                   │  │ │ Filter: [All]  │   │  │
│  │                   │  │ └───────────────┘   │  │
│  │                   │  │ ┌───────────────┐   │  │
│  │                   │  │ │ ● Bash 2.3s   │   │  │
│  │                   │  │ │ ● Edit 0.8s   │   │  │
│  │                   │  │ │ ❌ Bash 15.2s  │   │  │
│  │                   │  │ │ ✓ Read 0.1s   │   │  │
│  │                   │  │ └───────────────┘   │  │
│  └──────────────────┘  └───────────────────┘  │
└──────────────────────────────────────────────┘
```

### Data Model

```rust
struct TimelineEntry {
    id: u64,                     // sequential turn ID
    turn_index: u32,             // zero-based turn number
    timestamp: DateTime<Utc>,    // when the tool call started
    tool_type: ToolType,         // Bash, Read, Edit, SearchReplace, etc.
    status: EntryStatus,         // Running, Success, Error, Denied
    duration: Option<Duration>,  // None if still running
    summary: String,             // truncated result or error message
    detail: String,              // full result (viewable on expand)
    scrollback_line: u32,        // line number in scrollback for jump-to
}

enum ToolType {
    Bash,
    Read,
    Edit,
    SearchReplace,
    Grep,
    WebSearch,
    WebFetch,
    Task,
    Other(String),
}

enum EntryStatus {
    Running,
    Success,
    Error(String),
    Denied(String),
}
```

### TUI Component

The timeline is a `ratatui` `List` widget or a custom `ScrollablePanel`:

```
┌─── Timeline ──────────────────────────────────┐
│ [All] [Bash] [Edit] [Read] [✕ Errors Only]   │
│                                                │
│  #142  ● Bash          "cargo build --release" │
│        ✓ exit:0  2.3s   Compiled in release    │
│                                                │
│  #141  ✓ Edit          "src/main.rs:255-270"   │
│        ✓ success 0.8s  Applied 16-line edit    │
│                                                │
│  #140  ❌ Bash         "./deploy.sh prod"      │
│        ✗ exit:1  15.2s "Error: auth failed"   │
│                                                │
│  #139  ✓ Read          "Cargo.toml"            │
│        ✓ success 0.1s  42 lines                │
└────────────────────────────────────────────────┘
```

Controls:
- `Tab` / `Shift+Tab` to focus timeline panel.
- `j`/`k` or `↑`/`↓` to scroll timeline entries.
- `Enter` to jump scrollback to that entry.
- `f` to open filter menu: tool type, status, text search.
- `e` to expand/collapse entry detail.
- `Esc` to return focus to scrollback.

### Metrics Bar

At the top of the timeline:

```
Session: 142 turns | 23m 14s | Tools: ● 87 Bash ✓ 32 Edit ✓ 18 Read ❌ 5 Errors
```

### Persistence

Timeline entries persist for the session duration only. They are not stored between sessions (that's what snapshot #5 is for). However, the last N entries are always available in the current session.

## Implementation Plan

### Phase 1 — MVP (2-3 weeks)
1. Implement `TimelineStore` in-memory ring buffer (last 500 entries).
2. Define `TimelineEntry` and wire into agent tool call lifecycle (start → end).
3. Implement basic `List` widget rendering with color-coded tool types.
4. Implement jump-to-scrollback (map entry to scrollback line number).
5. Tests: entries created correctly, timeline shows N entries, jump-to works.

### Phase 2 — Filtering & Interaction (1-2 weeks)
1. Implement filter by tool type (checkbox list).
2. Implement "errors only" filter.
3. Implement text search across summaries.
4. Add expand/collapse for entry detail.
5. Tests: filter correctness, search matches, edge cases (no matching entries).

### Phase 3 — Metrics & Polish (1 week)
1. Implement metrics bar (turn count, total time, tool mix).
2. Add duration color coding (green < 1s, yellow < 5s, red > 5s).
3. Add keyboard navigation help tooltip.
4. Performance: virtualized rendering for 1000+ entries.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | TimelineEntry creation, filter predicates, search matching |
| Integration | Full turn → verify TimelineEntry matches actual tool call |
| Rendering | Timeline panel renders without overflow at 1/10/100/500 entries |
| Navigation | Jump-to-scrollback lands on the correct line |

## Open Questions

1. Should timeline support custom tool types from MCP servers? *Decision:* Yes, MCP tools appear as `Other(mcp_server_name)`. Future: custom tool type registration.
2. Should the timeline be collapsible (hidden by default, toggled with a keybind)? *Decision:* Yes, visible by default but toggleable with `Ctrl+T`.
3. Concurrency: how to display overlapping background tasks? *Decision:* Stack entries vertically with a slight left indent for nested tasks.
