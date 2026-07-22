# Spec 04 — Grok REPL Mode

- **Priority:** P1
- **Crate:** `xai-grok-repl` (new)
- **Depends on:** `xai-grok-shell-base`, `xai-grok-tools`, `xai-ratatui-textarea`
- **Status:** Draft

---

## Overview

Embed persistent language REPLs (Python, SQL, shell) directly inside the grok-build TUI session. Instead of the write-file → bash-run → read-output loop, users can interactively experiment with code snippets, query databases, or run shell pipelines with real-time output capture that feeds back into the agent's context.

## Motivation

The current workflow for exploratory coding is serial and slow: write a snippet to a temp file, run it via bash, read the output, realize you need to tweak, repeat. This breaks flow state. A REPL mode lets users (and the agent) iterate in milliseconds, with every expression and result captured as conversation context.

Common use cases:
- "Try this Python one-liner and show me the result"
- "Query the local SQLite database"
- "Ping a service to check if it's up before editing it"
- "Quickly transform some data with jq/awk/sed"

## Goals

- `grok repl python` — opens an interactive Python REPL in a modal panel.
- `grok repl sql [--db path/to/db.sqlite]` — SQL REPL for SQLite/Postgres/MySQL.
- `grok repl sh` — persistent shell with scrollback capture.
- Every expression/result is captured as a conversation message (user: expression, assistant: result).
- Full readline support (history, search, completion).
- Ctrl+C interrupts the running expression without killing the REPL.
- `multiline` mode for pasting multi-statement blocks.
- State persists across the session (variables, imports, connections).

## Non-Goals

- Full IDE debugger (breakpoints, stepping, watches — out of scope).
- Language server completions (future enhancement via existing LSP infra).
- REPL for compiled languages (Rust, Go, C — compile-run loop stays).

## Design

### Architecture

```
┌────────────────────────────────────┐
│  xai-grok-pager (TUI)              │
│  ┌──────────────────────────────┐  │
│  │ Main Scrollback               │  │
│  └──────────────────────────────┘  │
│  ┌──────────────────────────────┐  │
│  │ REPL Modal Panel              │  │
│  │ ┌──── input ───────────────┐ │  │
│  │ │ >>> import math           │ │  │
│  │ │ >>> math.pi               │ │  │
│  │ └──────────────────────────┘ │  │
│  │ ┌──── output ─────────────┐ │  │
│  │ │ 3.141592653589793       │ │  │
│  │ └──────────────────────────┘ │  │
│  └──────────────────────────────┘  │
└──────────┬─────────────────────────┘
           │
           ▼
┌────────────────────────────────────┐
│  xai-grok-repl                      │
│                                    │
│  ReplEngine trait:                 │
│  ┌──────────┐ ┌──────────┐ ┌────┐ │
│  │ Python   │ │ SQL      │ │ SH │ │
│  │ REPL     │ │ REPL     │ │    │ │
│  └──────────┘ └──────────┘ └────┘ │
│                                    │
│  ┌──────────────────────────────┐  │
│  │ History Manager               │  │
│  │ (per-REPL, persisted to       │  │
│  │  ~/.grok/repl_history/)       │  │
│  └──────────────────────────────┘  │
│  ┌──────────────────────────────┐  │
│  │ Context Capturer              │  │
│  │ (expression → conversation   │  │
│  │  message injection)          │  │
│  └──────────────────────────────┘  │
└────────────────────────────────────┘
```

### ReplEngine Trait

```rust
#[async_trait]
trait ReplEngine: Send + Sync {
    /// Name shown in the TUI ("py", "sqlite", "sh").
    fn name(&self) -> &'static str;

    /// Start the REPL subprocess.
    async fn start(&mut self) -> Result<()>;

    /// Evaluate a single expression, return stdout + stderr.
    async fn eval(&mut self, input: &str) -> Result<ReplResult>;

    /// Check if the REPL is still alive; restart if dead.
    async fn health_check(&mut self) -> Result<()>;

    /// Gracefully shut down.
    async fn shutdown(&mut self) -> Result<()>;
}

struct ReplResult {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    duration: Duration,
}
```

### Python REPL

- Backed by a subprocess running `python3 -iq` (interactive + quiet).
- Uses `pty` to present a real TTY to Python (so `colorama`, `rich`, `tqdm` work).
- Input is sent line-by-line. Multiline blocks (def, class, for) detected via Python's `code.InteractiveInterpreter` bracket matching, or sent with `...` continuation prompts.
- `pip install` within the REPL modifies the current environment.
- Virtual env detection: auto-activates `.venv/bin/python` if present.

### SQL REPL

- Backed by `sqlite3` for SQLite (local file), or `psql`/`mysql` client for remote databases.
- Connection string format: `grok repl sql sqlite://path/to/db.sqlite`, `grok repl sql postgres://user:pass@host/db`
- `.tables`, `.schema`, `.indexes` dot commands supported.
- Results rendered as a ratatui table widget.

### Shell REPL

- Persistent shell subprocess (bash/zsh/fish, auto-detected).
- Unlike `BashTool`, this REPL maintains state: `cd`, `export`, `source` persist between expressions.
- Full scrollback capture: every command and its output are stored in the conversation context (configurable, default: only the last 20 lines of output per command).

### TUI Integration

The REPL opens as a split-pane modal in the existing pager:

```
┌─────────────────────────────────────────────┐
│  Agent scrollback                            │
│  ...                                         │
│  [REPL: py] >>> import math                  │
│  [REPL: py] >>> math.pi                      │
│  [REPL: py] 3.141592653589793               │
│                                              │
├──────────────────────┬──────────────────────┤
│ REPL input           │ REPL output          │
│ >>> _                │ 3.141592653589793    │
│                      │                      │
└──────────────────────┴──────────────────────┘
```

Keybindings:
- `Ctrl+R` to open REPL prompt.
- `Ctrl+Enter` / `Alt+Enter` to evaluate (Enter for newline in multiline mode).
- `Ctrl+C` to interrupt.
- `Ctrl+D` to exit REPL.
- `Tab` for completions (Python: from `jedi`, SQL: table names).

### Context Capture

Every REPL interaction is captured as a conversation turn:

```json
{
  "role": "user",
  "content": "[REPL:py] >>> math.comb(10, 3)"
}
{
  "role": "assistant",
  "content": "[REPL:py] 120"
}
```

This way the model sees the full REPL session history and can reason about results.

### Config

```toml
[repl]
enabled = true
history_size = 1000
capture_context = true
max_output_lines = 100
python_path = "python3"
sqlite_path = "sqlite3"
```

## Implementation Plan

### Phase 1 — Python REPL MVP (2-3 weeks)
1. Create `xai-grok-repl` crate with `ReplEngine` trait.
2. Implement `PythonRepl` backed by pty subprocess.
3. Implement basic TUI modal (split pane with input/output).
4. Implement context capture (expression → conversation message).
5. Tests: eval round-trip, multiline input, pty allocation.

### Phase 2 — SQL & Shell REPLs (2-3 weeks)
1. Implement `SqlRepl` (SQLite first, Postgres/MySQL connection strings).
2. Implement `ShellRepl` (persistent bash subprocess).
3. Add result rendering: SQL tables as ratatui widgets.
4. Add readline + history per REPL type.
5. Tests: SQL query results, shell state persistence, connection management.

### Phase 3 — Polish (1-2 weeks)
1. Tab completions (jedi for Python, table names for SQL).
2. Ctrl+C interrupt handler.
3. Auto-restart on REPL crash.
4. `grok repl list` — show active REPLs with stats.
5. Performance: subprocess startup time, large output handling.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | ReplEngine eval/history/context capture |
| Integration | Python REPL: eval, multiline, error handling, pty |
| SQL | SQLite query returns correct table rendering |
| Shell | cd persists, env vars survive between evals |
| Resilience | REPL crash → auto-restart; Ctrl+C → clean state |

## Open Questions

1. Security: REPL runs outside the sandbox (bypasses bwrap). Should it respect sandbox profiles? *Decision:* Yes, REPL subprocesses are sandboxed like any other command.
2. Python dependencies: should `pip install` within REPL be allowed? *Decision:* Yes, but logged and gated by the same permission system as `BashTool`.
3. Q: How to handle REPLs in headless mode? *Decision:* REPL mode is disabled in headless/CI mode. Falls back to batch eval: `grok eval python "2+2"`.
