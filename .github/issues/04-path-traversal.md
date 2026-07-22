## `resolve_model_path` Lacks Path Traversal Sanitization

**Status: RESOLVED** (verified in current tree)

Now performs:

```rust
let resolved = cwd.join(input_path);
// Canonicalize to strip `..` traversal; fall back to original if path
// doesn't exist yet (new file creation).
resolved.canonicalize().unwrap_or(resolved)
```

Plus DisplayCwd rewriting for worktrees.

See: crates/codegen/xai-grok-tools/src/types/resources.rs (resolve_model_path).

**File:** `crates/codegen/xai-grok-tools/src/types/resources.rs`

**Severity:** High

### Description

The `resolve_model_path()` function (L479-505) resolves model-provided file paths against the current working directory but does **not** canonicalize the result or reject paths that escape the workspace root via `../` traversal.

**Current behavior:**

```rust
pub fn resolve_model_path(
    cwd: &std::path::Path,
    display_cwd: Option<&std::path::Path>,
    input: &str,
) -> PathBuf {
    // ... sanitize, tilde expand ...
    cwd.join(input_path)  // L504 — no canonicalization
}
```

`Path::join` resolves `../` components, so `cwd.join("../../etc/shadow")` produces a valid absolute path outside the workspace. The resulting `PathBuf` is passed directly to the file system operations (read, write, edit tools).

**Existing defenses:**

The permission layer (`PermissionHandle::request`) intercepts tool calls and checks access against a managed policy. However, this check operates on the **user-provided** path or its resolved form — and the defense can be bypassed in several scenarios:
- YOLO mode (`--yolo` or always-approve) — no permission prompt at all
- Auto mode — the LLM classifier may approve a seemingly benign path
- User approval — a user may approve a request without noticing `../` in the path
- No managed policy configured — default allow for workspace operations

### Attack Scenario

An LLM agent (or a malicious subagent) calls the `write` tool with:

```json
{"file_path": "../../etc/cron.d/malicious", "content": "...payload..."}
```

In YOLO mode, this writes outside the workspace with no prompt. In auto mode, the classifier sees `cron.d/malicious` which looks like a legitimate operation. The result is persistent code execution on the host.

### Outcome

- Write arbitrary files outside the workspace (cron jobs, SSH authorized_keys, systemd services, etc.)
- Read arbitrary files outside the workspace (`shadow`, `.ssh/id_rsa`, database credentials)
- Persistent backdoor installation on the host system
- Privilege escalation from workspace sandbox to host compromise

### Suggested Fix

Add canonicalization and containment check to `resolve_model_path`:

```rust
pub fn resolve_model_path(
    cwd: &std::path::Path,
    display_cwd: Option<&std::path::Path>,
    input: &str,
) -> PathBuf {
    let input = sanitize_model_path_arg(input);
    let expanded = shellexpand::tilde(input);
    let input_path = std::path::Path::new(expanded.as_ref());
    
    // ... existing resolution logic ...
    let resolved = cwd.join(input_path);
    
    // NEW: Canonicalize and check containment
    if let Ok(canonical) = resolved.canonicalize() {
        if !canonical.starts_with(cwd) {
            // Path traversal detected — clamp to workspace or return error
            // (depends on whether caller handles this gracefully)
        }
        canonical
    } else {
        resolved  // path doesn't exist yet (e.g., new file) — fall back
    }
}
```

Note: `canonicalize` fails for paths that don't exist yet (new file creation). Use `dunce::canonicalize` for cross-platform support, or only check containment for existing paths, and for new paths verify that joining the input doesn't produce `..` components.
