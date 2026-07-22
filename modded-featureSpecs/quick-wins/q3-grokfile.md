# Spec Q3 — Grokfile

- **Priority:** P1
- **Crate:** `xai-grok-config` (extend) / `xai-grok-shell` (discovery)
- **Status:** Draft

---

## Overview

A `Grokfile.toml` that lives at the project root and auto-configures grok-build when you `cd` into the directory. Contains project-level defaults for model, sandbox profile, MCP servers, environment variables, and startup commands.

## Motivation

Different projects need different agent configurations. A Rust backend project needs the `build-rust` sandbox profile and `cargo check` on file save. A Node.js frontend project needs the `web-dev` sandbox profile and `npm run dev` in the background. Currently, users must manually switch configs or use global settings.

A `Grokfile.toml` makes agent configuration part of the project — checked in to version control, shared with the team, and automatically applied when `grok` starts in that directory.

## Design

```toml
# Grokfile.toml — auto-loaded when grok starts in this directory
[project]
name = "my-api"
version = "1.0.0"

[grok]
model = "grok-3"
sandbox_profile = "build-rust"

[mcp_servers]
filesystem = { command = "npx", args = ["@modelcontextprotocol/server-filesystem", "."] }
github = { command = "npx", args = ["@modelcontextprotocol/server-github"] }

[background_tasks]
dev = { command = "cargo watch -x check", auto_start = true }
db = { command = "docker compose up db", auto_start = false }

[env]
RUST_LOG = "info"
DATABASE_URL = "postgres://localhost:5432/myapp"

[hooks]
install = true  # auto-install .grok/hooks/ on session start

[workspace]
warn_on_untracked = true
ignore_patterns = ["target/", "node_modules/", ".env"]
```

### Loading Order

1. `~/.grok/config.toml` (user global)
2. `Grokfile.toml` (project, checked in to VCS)
3. `.grok/config.toml` (project-local, not checked in, overrides Grokfile)

This mirrors the existing requirement > user > managed config chain.

### CLI

```
grok init                 # also generates Grokfile.toml
grok config show          # shows effective config (merged from all layers)
grok config grokfile      # show the Grokfile's raw contents
```

## Implementation

1. Add `Grokfile.toml` parsing to `xai-grok-config` (reuse existing TOML config infra).
2. Add discovery: on session start, look for `Grokfile.toml` from cwd upward.
3. Merge Grokfile with global config (Grokfile values are overridable by `.grok/config.toml`).
4. Add `background_tasks` auto-start on session start.
5. Add `hooks.install` auto-install logic.
