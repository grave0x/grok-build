## `persisted_bash_auto_allows` Can Auto-Approve Undecomposable Scripts

**Status: RESOLVED**

In `bash_grant_pre_decision`, `Unparseable` now respects `conservative_blanket: true` (set in `post_classify`), so it requires exact `allowed_bash_commands` match and never uses blanket `allow_bash_execute`.

Code comment explicitly calls out the protection for undecomposable scripts.

See: crates/codegen/xai-grok-workspace/src/permission/manager.rs (evaluate_bash_segments, BashGrantOpts::post_classify, Unparseable arm).

**File:** `crates/codegen/xai-grok-workspace/src/permission/manager.rs`

**Severity:** Medium

### Description

When the tree-sitter shell parser cannot decompose a command into segments (e.g., due to heredocs, `$(…)` command substitution, backtick expressions), the segment evaluator returns `SegmentEvaluation::Unparseable`. In the `bash_grant_pre_decision` path (L756-770), this triggers a decision path that can auto-approve the entire script based on blanket grants:

```rust
SegmentEvaluation::Unparseable => {
    if !opts.allow_blanket { return None; }
    let allowed = if opts.conservative_blanket {
        state.allowed_bash_commands.contains(cmd)       // exact match only
    } else {
        persisted_bash_auto_allows(state, cmd, yolo_pin) // blanket approval!
    };
    if allowed {
        grant_allow(reasons::SESSION_GRANT)
    } else {
        None
    }
}
```

The critical path is when `conservative_blanket = false` (post-classifier mode, L709-715). In this mode:

```rust
fn post_classify(auto_forced_prompt: bool) -> Self {
    Self {
        honor_safe_lists: true,
        allow_blanket: !auto_forced_prompt,  // true when classifier did NOT force a prompt
        conservative_blanket: false,
    }
}
```

And `persisted_bash_auto_allows` (L675-681) checks:
```rust
fn persisted_bash_auto_allows(state, cmd, yolo_pin) -> bool {
    (state.allow_bash_execute && yolo_pin.is_none()) 
    || state.allowed_bash_commands.contains(cmd)
}
```

The `state.allow_bash_execute` flag is a persisted blanket "allow all bash" that survives restarts (until the migration at L954-960 catches it). If a user previously approved "allow all bash" and the parser can't decompose a subsequent script, it runs without any segment-level checking.

### Attack Scenario

1. User approves a "remember for this session" on a bash command that sets `allow_bash_execute` (or has a legacy persisted Allow from migration)
2. Agent sends a complex script using heredocs: `cat <<EOF | bash\nrm -rf /important\nEOF`
3. Tree-sitter fails to parse the heredoc structure, returns `Unparseable`
4. `persisted_bash_auto_allows` returns `true` due to the blanket flag
5. The destructive script executes without any user prompt or segment-level deny-rule check

### Outcome

- Undecomposable scripts bypass segment-level deny rules (`rm`, `chmod`, `git push` are not checked per-segment)
- Scripts that the parser can't understand are trusted implicitly based on a stale permission flag
- Users who previously granted "allow bash" (expecting the tool to still check individual commands) lose that protection for complex scripts

### Suggested Fix

1. For `Unparseable` scripts, always require the user to approve the full raw script text, even under `allow_bash_execute` — blanket approval should not cover scripts the parser cannot decompose
2. Show the raw script to the user with a warning: "This command is complex and could not be fully analyzed for safety"
3. Consider making `conservative_blanket = true` the default for post-classifier as well, so only exact-match `allowed_bash_commands` entries (not blanket approval) pass unparseable scripts
