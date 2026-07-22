# Spec Q1 — Hooks Git-Tracking

- **Priority:** P2
- **Crate:** `xai-grok-hooks` (extend existing)
- **Depends on:** `xai-grok-workspace`, `xai-gix-status`
- **Status:** Draft

---

## Overview

Enable version-controlling `.grok/hooks/` alongside your project code. Hooks are automatically loaded from the repo root's `.grok/hooks/` directory (project-scoped). This spec adds `grok hooks install`/`grok hooks sync` commands and a `HooksTrust` file for managing hook trust policies.

## Motivation

Currently, `~/.grok/hooks/` is global. There's no standard way to ship hooks with a project repository. Teams want to share hook policies (safe-shell guards, audit logging) via version control.

## Design

- `grok hooks init` — create `.grok/hooks/` structure with a `README.md`.
- `grok hooks install` — symlink or copy project hooks from `.grok/hooks/` to the active hooks dir.
- `grok hooks sync` — pull latest hooks from the project's `.grok/hooks/` and reinstall.
- `.grok/hooks-trust` file: a checked-in allowlist of hook scripts that are safe to run:

```
# .grok/hooks-trust
# Format: sha256 <filename> <description>
sha256 abc123... safe-shell-guard.sh   "Blocks rm -rf /"
sha256 def456... session-log.sh        "Logs session metadata"
```

- `grok hooks verify` — checks that installed hook scripts match their `hooks-trust` hashes.
- On clone, `grok` auto-discovers `.grok/hooks/` and prompts to install (unless `hooks-trust` is present and trusted).

## Implementation

Extends `xai-grok-hooks`:
1. Add install/sync/verify commands to the discovery module.
2. Add `hooks-trust` file parsing and hash verification.
3. Add first-run detection of `.grok/hooks/` on session start.
