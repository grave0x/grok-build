# Spec 12 — Cost Analytics

- **Priority:** P1
- **Crate:** `xai-grok-cost` (new)
- **Depends on:** `xai-grok-models`, `xai-grok-sampler`, `xai-grok-config-types`, `xai-sqlite-journal`
- **Status:** Draft

---

## Overview

Track, estimate, and report API costs for grok-build sessions. Provides per-session cost breakdown, per-model spending, budget alerts, and exportable cost reports. Helps users understand and control their API spending.

## Motivation

AI API costs add up quickly. Users often have no idea how much a session costs until the bill arrives. Cost analytics provides:
- **Visibility:** what costs what, in real-time.
- **Budget control:** alerts when approaching a spending limit.
- **Optimization:** identify which models and tools consume the most budget.
- **Accountability:** cost-per-feature, cost-per-session for teams.

The existing `xai-token-estimation` crate estimates token counts. This spec extends that to actual cost tracking and reporting.

## Goals

- Track input + output tokens per model per session.
- Convert tokens to estimated cost using known pricing tables.
- `grok cost` — show current session cost summary.
- `grok cost report --period week` — export cost report as JSON/CSV.
- Budget alerts: warn via hook when approaching a daily/weekly/monthly spend limit.
- Per-project cost tracking (if multiple projects share the same credentials).
- Per-model cost breakdown: how much did grok-3 cost vs grok-3-fast?

## Non-Goals

- Actual billing integration (we estimate based on published prices; actual billing is on xAI's side).
- Invoice generation (out of scope for a CLI tool).
- Multi-user cost allocation (single-user cost tracking only).

## Design

### Architecture

```
┌──────────────────────────────────┐
│  xai-grok-sampler                 │
│  (completes a model call)         │
│  → emits CostEvent                │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  xai-grok-cost                    │
│                                  │
│  ┌────────────────────────────┐  │
│  │ Cost Tracker                │  │
│  │ (per-session counters)      │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Pricing Table               │  │
│  │ (model → $/token)          │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Budget Manager              │  │
│  │ (thresholds, alerts)       │  │
│  └────────────────────────────┘  │
│  ┌────────────────────────────┐  │
│  │ Persistent Store           │  │
│  │ (SQLite journal)           │  │
│  └────────────────────────────┘  │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  CLI: grok cost [...]            │
│  Hook: BudgetAlert event         │
└──────────────────────────────────┘
```

### Data Model

```rust
struct CostEvent {
    session_id: Uuid,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,  // cached reads are cheaper
    timestamp: DateTime<Utc>,
    tool_name: Option<String>, // which tool triggered this model call
}

struct CostBreakdown {
    session_id: Uuid,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cached_tokens: u64,
    estimated_cost_usd: f64,
    per_model: HashMap<String, ModelCost>,
    per_tool: HashMap<String, f64>,
}

struct ModelCost {
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
    cost_usd: f64,
}

struct BudgetState {
    daily_spend: f64,
    weekly_spend: f64,
    monthly_spend: f64,
    daily_limit: Option<f64>,
    weekly_limit: Option<f64>,
    monthly_limit: Option<f64>,
    alert_triggered: bool,    // prevent repeated alerts
}
```

### Pricing Table

Built-in pricing for xAI models (configurable for custom providers):

| Model | Input $/1M tokens | Output $/1M tokens | Cached $/1M tokens |
|-------|-------------------|-------------------|---------------------|
| grok-3 | 3.00 | 15.00 | 0.30 |
| grok-3-fast | 1.50 | 7.50 | 0.15 |
| grok-3-reasoning | 6.00 | 30.00 | 0.60 |
| Custom | configurable | configurable | configurable |

Prices can be overridden in config:

```toml
[cost.pricing]
"grok-3" = { input_per_mtok = 3.0, output_per_mtok = 15.0, cached_per_mtok = 0.3 }
"grok-3-fast" = { input_per_mtok = 1.5, output_per_mtok = 7.5, cached_per_mtok = 0.15 }
```

### CLI Interface

```
grok cost                              # current session cost summary
grok cost --verbose                    # per-model breakdown
grok cost report --period session      # export current session (JSON)
grok cost report --period today        # all sessions today (CSV)
grok cost report --period week         # this week
grok cost report --period month        # this month
grok cost report --period "2025-01"    # specific month
grok cost report --format json|csv     # output format
grok cost budget                       # show budget state
grok cost budget set daily 5.00        # set $5/day budget
grok cost budget clear                 # clear all budgets
grok cost watch                        # real-time cost ticker (TUI overlay)
```

### Config

```toml
[cost]
enabled = true
store_path = "~/.grok/cost.db"         # persistent cost database

[cost.budget]
daily = 5.0                            # $5/day soft limit
weekly = 25.0                          # $25/week
monthly = 100.0                        # $100/month
alert_threshold = 0.8                  # warn at 80% of budget
```

### Hook Integration

```json
{
  "hooks": {
    "BudgetAlert": [
      {
        "hooks": [
          { "type": "command", "command": "bin/notify-budget.sh" }
        ]
      }
    ]
  }
}
```

## Implementation Plan

### Phase 1 — Core Tracker (1-2 weeks)
1. Create `xai-grok-cost` crate with `CostEvent`, `CostTracker`, `CostBreakdown`.
2. Implement in-memory cost tracking (per session).
3. Implement pricing table.
4. Wire into `xai-grok-sampler` to receive model call completion events.
5. Implement `grok cost` command.
6. Tests: cost calculation matches known inputs, pricing table lookup.

### Phase 2 — Persistence & Reporting (1-2 weeks)
1. Implement SQLite persistent store (`CostDb` using `xai-sqlite-journal`).
2. Implement `grok cost report` with time range filtering.
3. Implement CSV and JSON export.
4. Tests: persistence across sessions, time range filtering, export format correctness.

### Phase 3 — Budget Alerts (1 week)
1. Implement `BudgetManager` with daily/weekly/monthly limits.
2. Implement budget alert hook event.
3. Implement `grok cost budget` subcommands.
4. Implement `grok cost watch` TUI overlay.
5. Tests: budget threshold firing, daily/weekly/monthly rollover.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Cost calculation, pricing table lookup, budget threshold logic |
| Integration | Mock sampler → correct cost events → report matches expected values |
| Persistence | Cost events survive process restart → report still accurate |
| Alert | Budget threshold reached → hook fires → agent receives alert |

## Open Questions

1. Should pricing be fetched from an API or hardcoded? *Decision:* Hardcoded with config override. Prices rarely change and the config override handles custom/bespoke models.
2. How to handle free tier / credits? *Decision:* Configurable `cost.free_credits_usd = 25.0` that is subtracted from the total.
3. Should budget alerts be blocking (deny further model calls) or non-blocking (warn only)? *Decision:* Non-blocking in MVP. Blocking mode optional in Phase 3.
