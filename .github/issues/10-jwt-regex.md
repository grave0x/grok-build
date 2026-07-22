## JWT Regex Can Redact Non-JWT Content

**Status: RESOLVED** (Turn 1 + Turn 2 complete)

Regex uses `{4,}`. Added `is_likely_jwt()` that base64-decodes the header segment and validates it parses as JSON via `serde_json::from_slice`. Non-JSON eyJ triples are no longer redacted.

Added `base64` dependency. Changed `JWT_REGEX.replace_all` to use a closure with `is_likely_jwt` validation. Updated false-positive test to expect NO redaction on non-JSON base64. Added `valid_jwt_still_redacts` positive test.

See: crates/codegen/xai-grok-secrets/src/sanitizer.rs (`is_likely_jwt()` + `JWT_REGEX` replace_all closure). 

Turn 1: Review (code + searches), Testing (existing + new false-positive test), Refactoring (lowered `{8,}` → `{4,}`, added explanatory comment), Exploit attempt (crafted non-JWT example).
Turn 2: Added `base64` dependency, `is_likely_jwt()` base64-decode + JSON validation, closure-based replace_all, updated tests to verify non-JSON rejection and valid-JWT retention.

**File:** `crates/codegen/xai-grok-secrets/src/sanitizer.rs`

**Severity:** Low

### Description

The JWT pattern used in the secrets sanitizer (L34-35) is intentionally broad to catch deployment keys and OIDC tokens:

```rust
static JWT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| compile(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b"));
```

This matches any string that:
- Starts with `eyJ` (the base64url encoding of `{"`)
- Has 8+ alphanumeric/underscore/dash chars
- Has a `.`
- Has 8+ more such chars
- Has another `.`
- Has 8+ more such chars
- Ends at a word boundary

**The problem:** This matches ANY base64url-encoded triple starting with `eyJ`, not just valid JWTs. Examples of non-JWT content that triggers redaction:

- A JSON-literal `{"key":"value"}.next.segment` in log output
- Base64-encoded content that happens to start with `eyJ` and contain dots
- Debug output showing parsed JWT-like structures from the code itself

This is a false-positive risk (redacts content that's not a secret) rather than a false-negative risk (misses actual secrets). The regex is also not anchored — it uses `\b` at start and end, but `eyJ` doesn't start with a word-character boundary issue (since `e` is a word char, `\b` before it requires a non-word char before `e`, which is usually true but could mismatch in some edge cases like at the very start of input).

Additionally, the regex doesn't validate:
- That the first segment decodes to valid JSON with a `typ` or `alg` field
- That the segments are valid base64url (not just base64url chars)
- That there are exactly 2 dots (the `.` in `{8,}` can match anywhere, and `{8,}` for the last segment means a segment with fewer than 8 chars is missed, but also a `.` in the segment body doesn't end it)

### Outcome

- Debug logs showing JWT-like structures get the "token" portion redacted even though it's not a real token
- Code output containing embedded JSON starting with `eyJ` is garbled after redaction
- Hard to debug JWT-related issues because legitimate debug output is redacted
- User trust in the redaction system erodes if it produces obviously wrong redactions

### Suggested Fix

1. **Validate the first segment base64-decodes to JSON**: If the decoded bytes don't start with `{`, it's not a JWT header
2. **Match exactly 2 dots**: Anchor to exactly three dot-separated segments, not just "some dots somewhere"
3. **Lower the minimum segment length**: `{8,}` on the last segment misses short signatures/trailers; `{4,}` is more appropriate for the final segment and `{4,}` for the first two (some JWT headers can be short)
4. **Use a multi-step check**: First find candidate matches via regex, then validate by attempting base64 decode of the header

```rust
// Stricter JWT detection:
// Match exactly "word.eyJ<base64url>.<base64url>.<base64url>"
// Then validate the header decodes to JSON
fn is_valid_jwt(s: &str) -> bool {
    let parts: Vec<&str> = s.splitn(3, '.').collect();
    if parts.len() != 3 { return false; }
    if !parts[0].starts_with("eyJ") { return false; }
    // Attempt base64 decode of header
    base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE, parts[0])
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some()
}
```
