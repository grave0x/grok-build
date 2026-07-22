# Spec 02 — Supply Chain SBOM Generator

- **Priority:** P1
- **Crate:** `xai-grok-sbom` (new)
- **Depends on:** `xai-grok-workspace`, `xai-grok-tools`
- **Status:** Draft

---

## Overview

Auto-generate a Software Bill of Materials (SBOM) from the project under agent supervision. Parses language-specific manifest files (`Cargo.toml`, `package.json`, `requirements.txt`, `go.mod`, `Gemfile`, `pom.xml`) and exports to industry-standard formats (SPDX 2.3, CycloneDX 1.5). Includes optional dependency vulnerability checking via OSV.dev API.

## Motivation

Supply chain attacks are the #1 attack vector in 2024-2025. Grok Build, as an AI agent that reads and modifies code, is in a unique position to:
1. Know exactly what dependencies a project uses (it already reads manifests to understand the codebase).
2. Warn when introducing a dependency with known vulnerabilities.
3. Produce an SBOM as a byproduct of normal operation — no separate tool needed.

The gravemod hardening plan focuses on runtime security. SBOM coverage closes the supply chain gap.

## Goals

- `grok sbom` — generate SBOM for current project root.
- `grok sbom diff <ref1> <ref2>` — SBOM diff between two commits/releases.
- Auto-detect project type (Rust, Node, Python, Go, Ruby, Java/Maven).
- Export SPDX 2.3 JSON and CycloneDX 1.5 JSON.
- Optional `PreToolUse` hook: warn when `write`/`edit` adds a dependency with known CVEs.
- Dependency tree output: `grok sbom tree`.

## Non-Goals

- Dependency resolution (running `cargo metadata`, `npm ls`, etc. is delegated to existing tools).
- License compliance verification (SBOM includes license fields but doesn't enforce).
- SBOM signing (future work).

## Design

### Architecture

```
┌──────────────────┐     ┌──────────────────────┐
│  Project Root     │────▶│  xai-grok-sbom        │
│  (workspace)      │     │                       │
└──────────────────┘     │  ┌─────────────────┐  │
                         │  │ Manifest Detector│  │
                         │  │ (Cargo.toml →    │  │
                         │  │  cargo metadata) │  │
                         │  └─────────────────┘  │
                         │  ┌─────────────────┐  │
                         │  │ Dependency       │  │
                         │  │ Resolver         │  │
                         │  └─────────────────┘  │
                         │  ┌─────────────────┐  │
                         │  │ SBOM Serializer  │  │
                         │  │ (SPDX/CycloneDX) │  │
                         │  └─────────────────┘  │
                         │  ┌─────────────────┐  │
                         │  │ Vulnerability    │  │
                         │  │ Checker (OSV.dev)│  │
                         │  └─────────────────┘  │
                         └──────────────────────┘
```

### Manifest Detectors

| Language | Detector | Resolution Command | Parser |
|----------|----------|--------------------|--------|
| Rust | `Cargo.toml` | `cargo metadata --format-version 1` | JSON |
| Node/JS | `package.json` | Optional: `npm ls --json --all` | JSON |
| Python | `requirements.txt`, `pyproject.toml`, `Pipfile` | Optional: `pip freeze` | INI/TOML |
| Go | `go.mod` | `go mod graph` | Text |
| Ruby | `Gemfile` | `gem list` | Text |
| Java/Maven | `pom.xml` | Optional: `mvn dependency:tree` | XML |

### Data Model

```rust
struct Sbom {
    format: SbomFormat,          // Spdx | CycloneDx
    spec_version: String,
    metadata: SbomMetadata,
    packages: Vec<Package>,
    relationships: Vec<Relationship>,
}

struct Package {
    name: String,
    version: String,
    purl: String,                // Package URL (pkg:cargo/serde@1.0.0)
    license: Option<String>,     // SPDX license identifier
    checksum: Option<String>,    // BLAKE3 of the package archive
    source_url: Option<String>,  // VCS URL
    is_transitive: bool,         // direct vs transitive dependency
}

struct Vulnerability {
    id: String,                  // CVE-2025-12345 or GHSA-xxxx
    package_name: String,
    affected_versions: String,
    severity: Option<String>,    // CRITICAL | HIGH | MEDIUM | LOW
    description: String,
    fixed_version: Option<String>,
}
```

### CLI Interface

```
grok sbom                                      # auto-detect and generate SBOM
grok sbom --format cyclonedx                    # explicit format
grok sbom --output sbom.spdx.json              # write to file
grok sbom tree                                 # dependency tree (textual)
grok sbom diff HEAD~1 HEAD                      # SBOM diff between commits
grok sbom check                                # check for vulnerable dependencies
grok sbom check --threshold high               # only report HIGH+CRITICAL
grok sbom watch                                # watch mode: re-check after every edit
```

### Hook Integration

A `PreToolUse` hook that fires on `write`/`edit`/`search_replace`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "(Write|Edit|SearchReplace)",
        "hooks": [
          { "type": "xai:sbom-scan", "threshold": "high" }
        ]
      }
    ]
  }
}
```

When the hook detects a new dependency in the edited file, it runs a fast scan. If a known CVE is found, the hook returns `deny` with the vulnerability details.

### Config

```toml
[sbom]
enabled = true
default_format = "spdx"          # spdx | cyclonedx
vulnerability_check = true       # enable OSV.dev queries
vulnerability_threshold = "high" # minimum severity to warn on
cache_ttl = "24h"               # cache OSV.dev responses
```

## Implementation Plan

### Phase 1 — MVP (2 weeks)
1. Implement `Cargo.toml` detector + `cargo metadata` resolver.
2. Implement SPDX 2.3 JSON serializer.
3. Implement `grok sbom` (auto-detect → generate → stdout).
4. Implement `grok sbom tree` (textual dependency tree).
5. Tests: known Cargo project → valid SPDX output, dependency tree match.

### Phase 2 — Language Coverage (2-3 weeks)
1. Add `package.json` / `npm ls` detector.
2. Add `requirements.txt` / `pyproject.toml` detector.
3. Add `go.mod` / `go mod graph` detector.
4. Add CycloneDX 1.5 serializer.
5. Add `grok sbom diff`.
6. Tests: multi-language monorepo → correct aggregated SBOM.

### Phase 3 — Vulnerability Checking (1-2 weeks)
1. Implement OSV.dev API client with caching (`xai-sqlite-journal` for cache).
2. Implement `grok sbom check`.
3. Implement `PreToolUse` hook integration.
4. Add `grok sbom watch` mode.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Each manifest parser with known-good and malformed input |
| Integration | Full Rust project → valid SPDX, CycloneDX output; diff between versions |
| Vulnerability | Mock OSV.dev API → correct CVE detection, threshold filtering |
| Hook | Simulated edit that adds a vulnerable dep → hook denies correctly |
| Performance | Large project (500+ deps) → SBOM generation < 2 seconds |

## Open Questions

1. Should we bundle a local CVE database or always query OSV.dev? *Decision:* Cache OSV.dev results locally with configurable TTL. No bundled database.
2. How to handle private registries (not on crates.io/npmjs.com)? *Decision:* Include in SBOM with `source_url` if available, mark as `private` package.
3. License detection: from `Cargo.toml` `license` field, or run `cargo license`? *Decision:* Use manifest field + known mapping (SPDX identifiers). Don't run external license scanners.
