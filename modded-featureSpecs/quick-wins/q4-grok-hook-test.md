# Spec Q4 — `grok hook test`

- **Priority:** P2
- **Crate:** `xai-grok-hooks` (extend existing)
- **Status:** Draft

---

## Overview

Dry-run a hook file against a sample tool call to verify the matcher works and the script executes correctly — all without running an actual agent session.

## Motivation

Hook scripts are hard to debug. A regex typo in a matcher silently breaks the hook. A script may fail with an error code that's hard to diagnose. `grok hook test` lets developers iterate on hook files quickly:

1. Write/edit the hook JSON.
2. Run `grok hook test my-hook.json "Bash" '{"command":"rm -rf /"}'`
3. See if it matched, what the script output was, what decision it made.

## Design

```
grok hook test <hook-file> <tool-name> [params-json]

# Example:
grok hook test safe-shell.json Bash '{"command":"rm -rf /"}'
→ Matched: ✓
→ Decision: DENY
→ Reason: "Command matches dangerous pattern: rm -rf /"
→ Script output:
  {"decision":"deny","reason":"Command matches dangerous pattern: rm -rf /"}
→ Exit code: 2
→ Duration: 0.023s

grok hook test safe-shell.json Read '{"path":"src/main.rs"}'
→ Matched: ✗ (no matcher for "Read")
→ Decision: N/A (not processed)

grok hook test tool-logger.json Bash '{"command":"ls"}'
→ Matched: ✓
→ Decision: ALLOW (passive hook)
→ Script output:
  {"logged":true}
→ Duration: 0.015s
```

### Options

```
grok hook test <file> <tool> [params] [options]
  --timeout <secs>       script timeout (default: 5)
  --env KEY=VALUE        extra env vars for the script
  --verbose              show full script stdout/stderr
  --list-tools           list all tool names that have matchers in this file
```

### Implementation

1. Add `grok hook test` to `xai-grok-hooks` CLI (exposed via `xai-grok-build-cli`).
2. Reuse the existing hook matching and execution code path.
3. Add `--list-tools` to quickly scan matchers.
4. Print clear diagnostics for: no match, match + allow, match + deny, script error, timeout.
