## `bwrap_reexec_for_profile` Silently Returns `None` When bwrap Not in PATH

**Status: RESOLVED** (verified in current tree)

`bwrap_reexec_command` now checks `Command::new("bwrap").arg("--version").output()` early and returns None + eprintln if not available.

See: crates/codegen/xai-grok-sandbox/src/lib.rs (bwrap_reexec_command ~L240-260). Tests cover the behavior.

**File:** `crates/codegen/xai-grok-sandbox/src/lib.rs`

**Severity:** Medium

### Description

When a sandbox profile requires read-deny enforcement on Linux (e.g., a custom profile with `deny` paths), the system attempts to re-exec the process inside a `bwrap` mount namespace via `bwrap_reexec_for_profile()` (L462-478). This function calls `bwrap_reexec_command()`, which constructs a `std::process::Command::new("bwrap")`.

If `bwrap` is **not installed** or **not in PATH**, `Command::new("bwrap")` does NOT fail at construction time — it stores the program name and fails later at `.spawn()` time. However, `bwrap_reexec_command` never calls `.spawn()` — it returns `Some(cmd)` where `cmd` is a `Command` that will fail when executed. The caller (the shell's startup path) tries to spawn it and either gets an error or silently falls back.

The critical path is in `bwrap_reexec_command` (L243-276):

```rust
pub fn bwrap_reexec_command(
    deny_write: &[&str],
    deny_read: &[&str],
) -> Option<std::process::Command> {
    // ...
    let mut cmd = std::process::Command::new("bwrap");
    cmd.arg("--bind").arg("/").arg("/");
    // ... add bind mounts ...
    cmd.arg("--").arg(self_exe).args(args);
    Some(cmd)  // Returns Some even if bwrap doesn't exist!
}
```

When the caller tries to `exec()` this command, it fails with "No such file or directory". Depending on how the caller handles this, the process either:
- Logs a warning and continues WITHOUT sandbox enforcement (silent degradation)
- Crashes with an opaque error message

### Outcome

- Custom sandbox profiles with deny paths silently fail open on systems without bwrap installed
- Administrator configures a `strict` or custom deny profile expecting file access restrictions, gets no enforcement
- Inconsistent behavior: macOS gets Seatbelt enforcement (when available), Linux silently degrades
- The `has_globs` flag at L472 means a profile with deny globs AND missing bwrap returns `None` before the bwrap check — so the behavior is inconsistent between exact-path denies and glob denies

### Suggested Fix

1. In `bwrap_reexec_command`, check that `bwrap` is available at construction time:
   ```rust
   fn bwrap_available() -> bool {
       std::process::Command::new("bwrap")
           .arg("--version")
           .output()
           .is_ok()
   }
   ```
   Return `None` with a clear log message when bwrap is not found.

2. In the shell startup path, when `bwrap_reexec_for_profile` returns `None` but `requires_read_deny` is true, **fail closed** — refuse to start with an error message like: "Sandbox profile requires read-deny enforcement but bwrap is not installed. Install bwrap or choose a different sandbox profile."

3. Update the startup log message to distinguish "no sandbox needed" from "sandbox required but unavailable".
