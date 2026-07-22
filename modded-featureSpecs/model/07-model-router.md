# Spec 07 — Model Router

- **Priority:** P1
- **Crate:** `xai-grok-model-router` (new)
- **Depends on:** `xai-grok-models`, `xai-grok-sampler`, `xai-grok-config-types`, `xai-grok-tools-api`
- **Status:** Draft

---

## Overview

Intelligent per-task model selection that routes different types of work to different models based on capability, cost, and latency requirements. Enables using cheap-fast models for simple tasks (bash, read) and strong-reasoning models for hard tasks (planning, search_replace, code review).

## Motivation

Today, grok-build uses a single model per session. This is wasteful: simple operations like `ls` or `grep` don't need a 200B-parameter reasoning model, while complex multi-file refactors benefit from one. A model router slashes API costs by 40-60% and reduces latency for common operations without sacrificing quality on hard tasks.

## Goals

- Configurable routing table: `[model_routing.bash] model = "grok-3-fast"`.
- Automatic fallback: if preferred model is rate-limited or down, fall back gracefully.
- Cost tracker: per-session and per-model token/cost breakdown.
- Latency optimizer: fast models for interactive tasks, strong models for batch.
- `grok model status` — show current routing table and per-model usage.
- Zero-config sensible defaults: fast model for bash/read/list_dir, strong model for edit/search_replace/plan.

## Non-Goals

- Model-as-judge routing (don't ask one model to decide which model to use — too expensive).
- Real-time model switching mid-reasoning (switching happens between tool calls).
- Custom model fine-tuning (routing is about selection, not training).

## Design

### Architecture

```
┌─────────────────────────────┐
│  xai-grok-shell (agent)      │  Determines next action
└──────────┬──────────────────┘
           │ route(tool_type, complexity_hint)
           ▼
┌─────────────────────────────┐
│  xai-grok-model-router       │
│                             │
│  ┌───────────────────────┐  │
│  │ Routing Table          │  │  Tool → Model mappings
│  │ (config + defaults)    │  │
│  └───────────────────────┘  │
│  ┌───────────────────────┐  │
│  │ Complexity Estimator   │  │  Heuristic: prompt length, file count, etc.
│  └───────────────────────┘  │
│  ┌───────────────────────┐  │
│  │ Fallback Chain         │  │  Model A → Model B → Model C
│  └───────────────────────┘  │
│  ┌───────────────────────┐  │
│  │ Cost Tracker           │  │  Per-model token & cost counters
│  └───────────────────────┘  │
└──────────┬──────────────────┘
           │ model_id
           ▼
┌─────────────────────────────┐
│  xai-grok-sampler            │  Sampling with selected model
└─────────────────────────────┘
```

### Routing Table

```toml
[model_routing]
# Default model for everything not matched below.
default = "grok-3"

# Tool-specific routing. The model name must be one from [models].
[model_routing.tools]
bash = "grok-3-fast"           # simple command output parsing
read = "grok-3-fast"           # reading files needs no reasoning
list_dir = "grok-3-fast"       # listing directories is trivial
grep = "grok-3-fast"           # search results need minimal interpretation
web_search = "grok-3"          # synthesis of search results needs reasoning
search_replace = "grok-3"      # edit accuracy matters
enter_plan_mode = "grok-3-reasoning"  # planning benefits from reasoning
exit_plan_mode = "grok-3"      # execution from plan
task = "grok-3"                # complex multi-step tasks
code_review = "grok-3-reasoning" # deep code analysis

# Per-model config overrides.
[model_routing.models."grok-3-fast"]
max_tokens = 4096
temperature = 0.1
context_window = 32768

[model_routing.models."grok-3"]
max_tokens = 16384
temperature = 0.3
context_window = 131072

[model_routing.models."grok-3-reasoning"]
max_tokens = 32768
temperature = 0.7
reasoning_effort = "high"
context_window = 131072
```

### Complexity Heuristic

For tools that can benefit from graduated model selection (e.g., `bash` is always simple, but `search_replace` varies):

```rust
fn estimate_complexity(context: &RouteContext) -> Complexity {
    let prompt_tokens = context.prompt_tokens();
    let files_involved = context.file_count();
    let turns_remaining = context.goals_iterations_remaining();

    match (prompt_tokens, files_involved, turns_remaining) {
        (0..=500, 0..=1, _) => Complexity::Simple,
        (501..=4000, 2..=5, _) => Complexity::Moderate,
        _ => Complexity::Complex,
    }
}

enum Complexity {
    Simple,    // → fast model
    Moderate,  // → default model
    Complex,   // → strong/reasoning model
}
```

### Fallback Chain

```rust
struct FallbackChain {
    primary: ModelId,
    secondary: ModelId,
    tertiary: ModelId,
    fallback_strategy: FallbackStrategy,
}

enum FallbackStrategy {
    Sequential,       // try primary → secondary → tertiary
    Random,           // pick one of the chain at random (load balancing)
    LowestCost,       // pick the cheapest available
}
```

Fallback triggers:
- Rate limit (429 response)
- Model unavailable (503)
- Timeout (model takes >60s to start generating)
- Context window exceeded (fall back to larger context model)

### Cost Tracker

```rust
struct CostTracker {
    model_counts: HashMap<ModelId, ModelUsage>,
    session_total: CostBreakdown,
}

struct ModelUsage {
    calls: u64,
    input_tokens: u64,
    output_tokens: u64,
    estimated_cost_usd: f64,
    total_duration: Duration,
}
```

### CLI Interface

```
grok model status                          # show routing table + per-model usage
grok model status --verbose                # include fallback counts, avg latency
grok model route bash "list files"         # show which model would be used for this input
grok model reset-counts                    # reset session cost counters
grok model cost-estimate                   # show estimated session cost
```

### Hook Integration

A `ModelSwitch` hook event fires when the router switches models between turns:

```json
{
  "hooks": {
    "ModelSwitch": [
      {
        "hooks": [
          { "type": "command", "command": "bin/log-model-switch.sh" }
        ]
      }
    ]
  }
}
```

## Implementation Plan

### Phase 1 — MVP (2-3 weeks)
1. Create `xai-grok-model-router` crate with `RouterConfig` and `RoutingTable`.
2. Implement tool-to-model resolution with fallback.
3. Implement `CostTracker`.
4. Wire into `xai-grok-sampler` — pass resolved model ID during sampling.
5. Implement `grok model status`.
6. Tests: routing table parsing, tool-to-model resolution, fallback chain.

### Phase 2 — Complexity & Polish (1-2 weeks)
1. Implement `ComplexityEstimator` heuristic.
2. Add complexity-based routing for `search_replace`, `task`.
3. Implement fallback on rate limit / timeout / model unavailable.
4. Implement `grok model route <tool> <input>`.
5. Tests: complexity estimation accuracy, fallback triggers.

### Phase 3 — Cost Awareness (1 week)
1. Implement `grok model cost-estimate` (uses per-model pricing from a known table).
2. Add budget alert: warn when projected cost exceeds threshold.
3. Add `grok model reset-counts` for per-session cost tracking.
4. Tests: cost estimate matches real API usage; budget alert fires correctly.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Routing table parsing, tool-to-model resolution, fallback chain |
| Integration | Mock sampler → verify correct model is called for each tool type |
| Fallback | Primary returns 429 → secondary is used → log event fires |
| Cost | Per-model token counts match actual API response usage |

## Open Questions

1. Should the router consider the *current conversation context* (e.g., if the last 5 turns were all bash, the model is probably loaded already)? *Decision:* Not in MVP. Future: keep-alive for frequently used models.
2. How to handle tool calls that are part of a larger multi-turn reasoning chain (e.g., plan mode → bash → edit → bash)? *Decision:* Each tool call is routed independently. The planning context is always visible to the selected model via the shared conversation history.
3. Should there be a manual override? `grok model use grok-3-reasoning` to pin a specific model for the rest of the session. *Decision:* Yes, Phase 2.
