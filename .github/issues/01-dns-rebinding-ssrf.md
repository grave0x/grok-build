## DNS Rebinding Bypass in HTTP Hook SSRF Validation

**Status: RESOLVED** (verified in current tree)

The mitigation is implemented:
- `validate_hook_url` (http.rs:91) resolves once via `tokio::net::lookup_host`, validates all addrs with `is_blocked_ip`.
- Returns `HookUrlAddrs::Dns(host, addrs)`.
- `run_http_hook` pins via `client_builder.resolve(host, *addr)` before `.post(url).send()`.
- Extensive SSRF unit tests exist.

See: crates/codegen/xai-grok-hooks/src/runner/http.rs (validate_hook_url, run_http_hook ~L155, client pin ~L226).

**File:** `crates/codegen/xai-grok-hooks/src/runner/http.rs`

**Severity:** High

### Description

The HTTP hook SSRF validation in `validate_hook_url()` has a classic TOCTOU (Time-of-Check Time-of-Use) vulnerability. The function resolves the hook URL's hostname to IP addresses and checks them against private/link-local/cloud-metadata blocklists, but does **not** pin those resolved addresses for the subsequent HTTP request.

**The gap:**

1. **L110-131** — `validate_hook_url()` calls `tokio::net::lookup_host()` and checks each resolved IP against `is_blocked_ip()`. If all IPs are public, it returns `Ok(())`.
2. **L226-236** — `reqwest::Client::post(url).send()` performs its **own** DNS resolution, completely independent of the validation step.

An attacker who controls DNS for the hook URL domain can exploit this race window: serve a public IP during validation, then switch to a private/169.254.169.254 (cloud metadata) IP for the actual request.

### Attack Scenario

1. A plugin or hook config sets `url = "https://evil-attacker-controlled-domain.example/hook"`
2. At validation time, the domain resolves to `1.2.3.4` (public) — passes `is_blocked_ip()`
3. Between validation and the actual HTTP POST, the attacker flips DNS to `169.254.169.254` (AWS metadata endpoint)
4. `reqwest` resolves the new IP and sends the hook payload to the metadata service
5. The attacker exfiltrates the hook envelope (which contains session ID, workspace root, and tool input data) to their infrastructure

### Outcome

- Exfiltration of sensitive session data (tool inputs, workspace paths, session IDs)
- SSRF to internal/cloud-metadata endpoints from the hook execution context
- Potential lateral movement if the hook server is on the same network as internal services

### Suggested Fix

Replace the two-phase validate-then-connect approach with a single atomic flow:

1. Resolve DNS to all addresses in `validate_hook_url`
2. Filter out any private/blocked addresses
3. If no addresses remain, reject
4. Pass the pre-resolved, filtered addresses to the HTTP client so it does NOT re-resolve

One approach: use reqwest's custom DNS resolver (`reqwest::dns::Resolve`) that returns only the pre-validated addresses, or construct the connection manually using the validated `SocketAddr` set.
