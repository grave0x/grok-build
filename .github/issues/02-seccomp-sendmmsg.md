## Seccomp Filter Misses `sendmmsg` and Other Network Syscalls

**Status: RESOLVED** (verified in current tree)

Blocked list now includes:
- SYS_sendmmsg, SYS_socket, SYS_socketpair + the originals.

See: crates/codegen/xai-grok-sandbox/src/child_net.rs (install_child_network_filter ~L44).

**File:** `crates/codegen/xai-grok-sandbox/src/child_net.rs`

**Severity:** High

### Description

The per-child seccomp BPF filter at `install_child_network_filter()` blocks 7 syscalls: `connect`, `bind`, `sendto`, `sendmsg`, `listen`, `accept`, `accept4`. However, it misses several related syscalls that can be used to bypass the network restriction.

**Blocked syscalls (L44-52):**
- `SYS_connect`, `SYS_bind`, `SYS_sendto`, `SYS_sendmsg`
- `SYS_listen`, `SYS_accept`, `SYS_accept4`

**Not blocked:**
- `SYS_sendmmsg` (Linux-specific — sends multiple messages in one call, can send data)
- `SYS_recvfrom`, `SYS_recvmsg`, `SYS_recvmmsg` (receive data)
- `SYS_socketpair` (creates connected socket pairs)
- `SYS_getsockname`, `SYS_getpeername` (introspection)

While `connect` is blocked (preventing new outbound connections), if a child process:
- Inherits an open socket fd from the parent (possible via fork)
- Creates a socket via `SYS_socket` (NOT blocked) and the fd is already connected somehow
- Uses `SYS_sendmmsg` to send data on that fd

...it can still transmit data without triggering the seccomp filter.

### Attack Scenario

A sandboxed child process that inherits a pipe/socket fd (e.g., from a `pipe()` or `socketpair()` created before fork) could use `SYS_sendmmsg` to write data to it, which a non-sandboxed sibling reads and forwards to the network. This creates an escape channel from the network-restricted child.

### Outcome

- Data exfiltration from network-restricted child processes via inherited fds + unblocked syscalls
- Undermines the sandbox's network isolation guarantee for child subprocesses

### Suggested Fix

1. Add `SYS_sendmmsg` to the blocked syscall list
2. Strongly consider blocking `SYS_socket` entirely — a sandboxed child has no legitimate need to create new sockets
3. Block `SYS_socketpair` to prevent local socket creation
4. Document that inherited fds should be closed before exec in sandboxed children
