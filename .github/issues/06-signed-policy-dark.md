## Signed Managed Policy Verification Inactive By Default

**Status: RESOLVED**

`EMBEDDED_DEPLOYMENT_CONFIG_PUBKEYS` is `&[]` (intentional for OSS "dark" build), `verification_active()` returns false. Good docs now exist on the const explaining it.

Added one-time `tracing::warn!` at load time when managed policy files exist but verification is inactive (in `signed_cache_compromised`).

Test fixture now exercises full sign/verify/persist/tamper/restore/re-verify pathway:
- `full_integration_sign_persist_verify_tamper_detect` — end-to-end: key injection → sign → persist → reload → verify → tamper → detection → restore → re-verify
- `first_keyed_deployment_no_sidecar_is_not_compromised` — no sidecar at first launch is treated as clean (not tampered)
- `no_sidecar_full_integration_flow` — missing sidecar produces correct error
- `verification_active_with_fake_keys` — `verification_active()` returns true when keys injected

See: crates/codegen/xai-grok-config/src/signed_policy.rs (const + warning) + tests.rs (full test suite).

**File:** `crates/codegen/xai-grok-config/src/signed_policy.rs`

**Severity:** Medium

### Description

The signed managed policy system provides cryptographic verification of cloud-delivered configuration via Ed25519 signatures. However, the trusted key store is an **empty array** at compile time:

```rust
// L15 — compile-time constant
pub const EMBEDDED_DEPLOYMENT_CONFIG_PUBKEYS: &[(&str, &[u8])] = &[];

// L83-85
pub fn verification_active() -> bool {
    with_embedded_keys(|keys| !keys.is_empty())
}
```

Since the array is empty, `verification_active()` always returns `false`, and the entire signed policy pathway — signature verification, tamper detection, fail-closed enforcement, content-matching checks — compiles but **never executes**. The code is "dark" until someone customizes the binary with embedded public keys.

This is architecturally intentional for the open-source build (no secret keys shipped), but has sharp edges:

1. **No tamper detection**: Without signature verification, the on-disk managed policy can be freely edited, replaced, or deleted by anyone with filesystem access. The `policy_deny`, `policy_ask`, and `fail_closed` settings are effectively advisory, not enforced.
2. **No key rotation path tested**: Since the system is never active in the default build, the key rotation mechanism (`key_id` selection, sidecar format, expiry handling) has never been exercised in production-like conditions. A team that later injects keys may hit edge cases.
3. **Deployment surprise**: An ops team deploying this binary expecting managed-policy integrity (from reading the code) will get no protection unless they rebuild with keys injected. The code looks secure but isn't doing anything.

### Outcome

- Managed policy (requirements.toml, managed_config.toml) can be silently replaced by any user with filesystem write access
- Team admins who configure deny rules via cloud-delivered policies have no cryptographic guarantee they're enforced
- `fail_closed` mode compiles but never activates since the sidecar verification that gates it never runs
- First deployment that adds keys will face untested code paths for sidecar creation, verification, and failover

### Suggested Fix

1. **Document prominently**: Add a doc comment on `EMBEDDED_DEPLOYMENT_CONFIG_PUBKEYS` and in the crate-level docs explaining that managed policy signing is **inert until custom public keys are compiled in**. Link to a deployment guide for enterprise users.
2. **Add a startup warning**: When `verification_active()` is false and managed config files exist on disk, emit a one-time warning: "Managed policy loaded WITHOUT signature verification. Compile with EMBEDDED_DEPLOYMENT_CONFIG_PUBKEYS to enable integrity protection."
3. **Add a test fixture**: Create a test that injects test keys and exercises the full signature verification pathway (fetch, verify, persist, reload, tamper-detect) so the code is proven working even in the dark build.
