# Spec Q2 — `grok init` Template Generator

- **Priority:** P2
- **Crate:** `xai-grok-build-cli` (extend existing)
- **Depends on:** `xai-grok-config`, `xai-grok-mcp`
- **Status:** Draft

---

## Overview

Interactively scaffold `.grok/config.toml` with MCP servers, hooks, sandbox settings, and preferred models. A guided first-run experience for new users.

## Motivation

New grok-build users face a blank config file. Most don't know what MCP servers are available, what hooks do, or how to configure the sandbox. `grok init` provides an interactive TUI wizard that asks questions and generates a working config.

## Design

```
$ grok init
╭──────────────────────────────────────╮
│ Grok Build Configuration Generator    │
│                                      │
│ Project name: [my-project        ]   │
│                                      │
│ [x] Enable sandbox (bubblewrap)      │
│ Sandbox profile: [default ▾]         │
│     default, strict, build-rust      │
│                                      │
│ MCP Servers:                         │
│   [x] Filesystem (local file access) │
│   [x] GitHub (PRs, issues)           │
│   [ ] Database (Postgres)            │
│   [ ] Custom server...               │
│                                      │
│ Hooks:                               │
│   [x] Safe-shell guard               │
│   [ ] Session audit log              │
│                                      │
│ Model preference:                    │
│   (○) grok-3 (balanced)              │
│   ( ) grok-3-fast (cheap & fast)     │
│   ( ) grok-3-reasoning (best)        │
│                                      │
│ [Generate] [Cancel]                  │
╰──────────────────────────────────────╯
```

Generates:
- `.grok/config.toml` with selected options
- `.grok/hooks/` with selected hook scripts
- `.grok/hooks-trust` with hashes of installed hooks
- `.grok/README.md` explaining the directory structure

## Implementation

1. Add `grok init` to `xai-grok-build-cli`.
2. Use `inquire` or `ratatui` for the interactive form.
3. Template rendering via `minijinja` (already a dependency of `xai-grok-tools`).
4. Hook script copying from built-in examples.
5. After generation, print next steps: "Run `grok` to start a session."
