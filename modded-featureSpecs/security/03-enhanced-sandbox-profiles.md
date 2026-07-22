# Spec 03 — Enhanced Sandbox Profiles

- **Priority:** P0
- **Crate:** `xai-grok-sandbox` (extend existing)
- **Depends on:** `xai-grok-config`, `xai-grok-hooks`, `xai-grok-shell-base`
- **Status:** Draft

---

## Overview

Extend the existing bwrap-based sandbox with per-project profiles, network proxy awareness, resource limits via cgroups v2, and a dry-run mode. Allows power users and enterprises to define fine-grained sandbox policies that go beyond the current binary on/off.

## Motivation

The existing sandbox (issue #02, #07 in the hardening plan) provides basic containment via bubblewrap. However, it lacks:
- **Granularity:** one sandbox configuration fits all. Different projects need different trust levels (e.g., a Rust crate from crates.io needs network for `cargo build`; an internal Python script should have no network at all).
- **Network control:** no per-command or per-project network allowlist/denylist.
- **Resource limits:** no CPU/memory/disk quotas. A runaway `cargo build` can starve the system.
- **Visibility:** no dry-run mode. Users can't see what would be blocked without actually running the command.

This spec extends the existing `xai-grok-sandbox` crate with these capabilities.

## Goals

- Per-project sandbox profiles in `.grok/config.toml`.
- Network allowlist/denylist per profile (hostname globs, IP ranges).
- cgroups v2 resource limits (CPU quota, memory max, IO weight).
- Proxy awareness: route sandboxed traffic through a configurable HTTP/SOCKS proxy.
- Dry-run mode: `grock sandbox check <command>` — show what would be blocked without executing.
- Sandbox violation events: log + optionally deny commands that violate profile rules.
- Backward-compatible: existing sandbox behavior unchanged when no profiles are configured.

## Non-Goals

- Windows sandbox (Seatbelt is existing; profile system extends to it in future).
- Full container runtime (not a Docker/k8s replacement).
- Filesystem encryption (sandbox isolates, doesn't encrypt).

## Design

### Architecture

```
┌─────────────────────────┐
│  xai-grok-shell          │  Agent calls spawn() with command
│  (spawn / run_cmd)       │
└──────────┬──────────────┘
           │
           ▼
┌─────────────────────────┐
│  xai-grok-sandbox        │  Checks profile from config
│                          │
│  ┌───────────────────┐  │
│  │ Profile Selector   │──│── Reads .grok/config.toml [sandbox.profiles.*]
│  └───────────────────┘  │
│  ┌───────────────────┐  │
│  │ Network Filter     │  │  Hostname/IP allow/deny matching
│  └───────────────────┘  │
│  ┌───────────────────┐  │
│  │ Cgroup Controller  │  │  cgroups v2 CPU/memory/IO
│  └───────────────────┘  │
│  ┌───────────────────┐  │
│  │ Proxy Injector     │  │  HTTP_PROXY / ALL_PROXY env injection
│  └───────────────────┘  │
│  ┌───────────────────┐  │
│  │ Bwrap Builder      │  │  Existing bwrap argument construction
│  └───────────────────┘  │
└──────────┬──────────────┘
           │
           ▼
      ┌──────────┐
      │  bwrap    │  Executed sandboxed process
      └──────────┘
```

### Config Schema

```toml
[sandbox]
enabled = true
default_profile = "default"

[sandbox.profiles.default]
# Inherits current sandbox behavior (bwrap on Linux, seatbelt on macOS)

[sandbox.profiles.strict]
network = false
read_write_paths = ["$PWD/src", "$PWD/tests"]
read_only_paths = ["$PWD", "/usr/lib", "/nix/store"]
tmpfs_size = "1G"
cpu_quota = "50%"        # cgroups v2: 50% of one core
memory_max = "2G"
io_weight = 100
proxy = "http://127.0.0.1:8080"  # optional proxy

[sandbox.profiles.build-rust]
network_allow = [
    "crates.io",
    "static.crates.io",
    "github.com",
    "*.rust-lang.org",
]
network_deny = [
    "*.internal.*",
    "10.*",
    "172.16.*",
    "192.168.*",
]
network = true           # allowed but filtered
cpu_quota = "200%"       # 2 cores
memory_max = "8G"
read_only_paths = ["/nix/store", "/usr/lib"]
tmpfs_size = "2G"

[sandbox.profiles.web-dev]
network = true
network_allow = ["*"]    # unrestricted
proxy = "http://127.0.0.1:8080"  # route through local dev proxy
```

### Profile Selection Rules

Profiles are selected by:
1. **Explicit**: `sandbox.profile = "strict"` in project config.
2. **CWD-based**: `sandbox.profile_by_command.run_cargo = "build-rust"` — matches when command starts with `cargo`.
3. **Git root**: `sandbox.profile_by_repo."github.com/org/repo" = "strict"`.
4. **Default**: `sandbox.default_profile = "default"` (current behavior).

### Network Filter

```rust
struct NetworkFilter {
    allowlist: Vec<UrlPattern>,  // hostname globs, CIDR ranges
    denylist: Vec<UrlPattern>,
    default_policy: NetworkPolicy, // Allow | Deny
}

enum UrlPattern {
    Hostname(String),      // "crates.io"
    Glob(String),          // "*.rust-lang.org"
    Cidr(IpAddr, u8),      // "10.0.0.0/8"
    Regex(String),         // r"^.*\.internal\..*$"
}
```

The network filter works by:
1. Setting up a minimal DNS resolver inside the sandbox that resolves against an allowlist.
2. Using `bwrap --seccomp 9` (seccomp-bpf) to block `connect()` to denied IPs.
3. As a fallback: setting `http_proxy` / `https_proxy` / `no_proxy` env vars.

### Cgroups v2 Integration

```rust
struct CgroupConfig {
    cpu_quota: Option<String>,   // "50%", "200%", "2.5"
    memory_max: Option<String>,  // "2G", "512M"
    io_weight: Option<u8>,       // 1-10000
    pid_max: Option<u32>,        // max processes
}
```

On Linux:
1. Create a child cgroup under the agent's cgroup: `/sys/fs/cgroup/<agent-cgroup>/sandbox/<cmd-id>/`
2. Write limits to `cpu.max`, `memory.max`, `io.weight`, `pids.max`.
3. Move child process into the cgroup before exec.
4. Clean up cgroup on process exit.

### Proxy Awareness

```rust
struct ProxyConfig {
    http_proxy: Option<String>,   // "http://127.0.0.1:8080"
    https_proxy: Option<String>,  // "http://127.0.0.1:8080"
    no_proxy: Option<Vec<String>>,// ["localhost", "127.0.0.1", "*.local"]
    socks_proxy: Option<String>,  // "socks5://127.0.0.1:1080"
}
```

When a proxy is configured:
1. Inject `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`, `ALL_PROXY` env vars into the sandbox.
2. **New:** add a `proxy_test` subcommand that verifies the proxy is reachable and working.

### Dry-Run Mode

```
grok sandbox check "rm -rf /"          # → DENIED: matches sandbox.deny[0] "rm -rf"
grok sandbox check "cargo build"       # → ALLOWED (profile: build-rust)
                                       #   Network: crates.io ✓, *.internal.* blocked
                                       #   Resources: 200% CPU, 8G RAM
grok sandbox check --verbose "curl http://10.0.0.1"  # → DENIED: IP in denylist 10.0.0.0/8
```

Dry-run evaluates the command against all profile rules and prints the result without actually executing or sandboxing. Uses a lightweight `SELFTEST` flag on the evaluator.

### CLI Interface

```
grok sandbox status                    # show current sandbox state (enabled, profile, cgroups)
grok sandbox check <command>           # dry-run evaluation
grok sandbox profiles                  # list available profiles with summary
grok sandbox profiles strict           # show full profile details
grok sandbox cgroup stats              # show current cgroup resource usage
```

### Hook Integration

New hook event type: `SandboxViolation`. Fires when a command is denied by sandbox policy.

```json
{
  "hooks": {
    "SandboxViolation": [
      {
        "hooks": [
          { "type": "command", "command": "bin/log-violation.sh" }
        ]
      }
    ]
  }
}
```

The hook receives the command, profile name, matched rule, and reason.

## Implementation Plan

### Phase 1 — Profiles (2-3 weeks)
1. Extend `xai-grok-sandbox` config types with profile structs.
2. Implement profile loading from `.grok/config.toml`.
3. Implement profile selection logic (explicit, cwd-based, repo-based, default).
4. Wire profile into bwrap builder: add `read_write_paths`, `read_only_paths`, `tmpfs_size`.
5. Tests: profile selection, path mounting, fallback to default.

### Phase 2 — Network & Proxy (2-3 weeks)
1. Implement `NetworkFilter` with hostname glob and CIDR matching.
2. Implement DNS-level filtering via `bwrap --seccomp` or local DNS resolver.
3. Implement proxy env var injection.
4. Implement `grok sandbox check` (dry-run mode).
5. Tests: network allow/deny, proxy env injection, dry-run accuracy.

### Phase 3 — Cgroups & Polish (2-3 weeks)
1. Implement cgroups v2 controller (CPU, memory, IO, PID limits).
2. Implement cgroup cleanup on process exit.
3. Implement `grok sandbox status` and `grok sandbox cgroup stats`.
4. Implement `SandboxViolation` hook event.
5. Tests: cgroup limits enforced, cgroup cleanup on crash, violations logged.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Profile parsing, selection logic, network pattern matching |
| Integration | bwrap-launched `curl` blocked by network deny list; proxy env present |
| System | cgroups v2 limits verified via `/sys/fs/cgroup`; OOM kill confirmed |
| Dry-run | Check output matches actual sandbox behavior |
| Regression | Existing sandbox behavior unchanged when no profiles configured |

## Open Questions

1. Should profiles support inheritance? (e.g., `[sandbox.profiles.strict] inherits = "base"`) *Decision:* Yes, Phase 2.
2. macOS: how to map network filtering? Seccomp-bpf is Linux-only. *Decision:* Network filtering is Linux-only in MVP. macOS continues with existing seatbelt.
3. Cgroups v2: what if the host doesn't have cgroups v2 (older distros)? *Decision:* Graceful fallback: log warning, run without resource limits.
4. Should proxy config support authentication (user:pass)? *Decision:* Yes, via the URL format `http://user:pass@proxy:8080`. Stored in config (not world-readable).
