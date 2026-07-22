## `is_devbox_based` Uses `target_os` Gate for Pure Config Logic

**Status: RESOLVED**

`#[cfg(target_os = "linux")]` gate removed from `is_devbox_based` — it's pure config logic with no OS-specific operations. Gate remains only on `bwrap_deny_plan` and `bwrap_reexec_command`.

Cross-platform warning added via `OnceLock` + `tracing::warn!` in `is_devbox_based` when devbox profile is selected on non-Linux: "Devbox /data write-deny requires bwrap (Linux only); /data will be writable on this platform". Fires once per process lifetime.

See: `is_devbox_based` lib.rs (ungated) + `#[cfg(not(target_os = "linux"))]` warn-once block.

**File:** `crates/codegen/xai-grok-sandbox/src/lib.rs`

**Severity:** Low

### Description

The `is_devbox_based()` function (L339-348) determines whether a sandbox profile is devbox-based by reading config and comparing string values — no OS-specific operations. Despite this, it's gated behind `#[cfg(target_os = "linux")]`:

```rust
#[cfg(target_os = "linux")]
fn is_devbox_based(profile: &ProfileName, config: &SandboxConfig) -> bool {
    match profile {
        ProfileName::Devbox => true,
        ProfileName::Custom(name) => {
            config.profiles.get(name).and_then(|p| p.extends.as_deref()) == Some("devbox")
        }
        _ => false,
    }
}
```

The function is called from `bwrap_deny_plan`, which is also `#[cfg(target_os = "linux")]`, so there's no compilation error. However:

1. **The `/data` write-deny is lost on non-Linux platforms.** The devbox profile on macOS (or future platforms) gets no read-only bind on `/data`, meaning the devbox isolation guarantee is Linux-only.
2. **Conceptual mis-gating:** The gate should be on the *capability* (bwrap feature/enforce), not the *platform*. On macOS, there's no bwrap, so there's no `/data` bind mount mechanic — but the function doesn't do anything bwrap-specific (it just reads strings).
3. **Future platform risk:** If grok is ever ported to a new Unix platform that supports bwrap-style bind mounts (or a different mechanism for /data write-deny), this gate would need to be found and updated, which is easy to miss.

### Scenario

A developer on macOS configures `profile = "devbox"`, expecting `/data` to be exposed read-only (as documented). Since `is_devbox_based` is compiled out on macOS, `bwrap_deny_plan` returns an empty `deny_write` list, and the `/data` directory is fully writable. No warning is emitted on macOS because `bwrap_deny_plan` is also compiled out entirely.

### Outcome

- Devbox profile `/data` write-protection is silently missing on macOS
- Profile behavior differs across platforms without clear documentation
- Config-level logic gated by target_os makes platform behavior hard to audit
- Adding a new platform requires updating `cfg` gates in multiple places

### Suggested Fix

1. Remove `#[cfg(target_os = "linux")]` from `is_devbox_based` — it's pure config logic
2. Keep the `cfg` gate only on `bwrap_deny_plan` and `bwrap_reexec_command` (which actually use bwrap-specific functionality)
3. Add a compile-time or runtime warning on macOS when devbox profile is selected: "Devbox /data write-deny requires bwrap (Linux only); /data will be writable on this platform"
4. Consider extracting `/data` write-deny into a platform-agnostic profile flag so it can be enforced by other mechanisms on non-Linux platforms (e.g., Seatbelt on macOS)
