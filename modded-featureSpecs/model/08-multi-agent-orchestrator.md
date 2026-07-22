# Spec 08 — Multi-Agent Orchestrator

- **Priority:** P2
- **Crate:** `xai-grok-orchestrator` (new)
- **Depends on:** `xai-grok-subagent-resolution`, `xai-grok-shell`, `xai-grok-workspace`, `xai-grok-tools`
- **Status:** Draft

---

## Overview

An advanced orchestrator that coordinates multiple subagent workflows beyond the existing `/best-of-n` and `/check-work` skills. Supports leader/follower patterns, critic loops, auto-specialization, and hierarchical decomposition of complex tasks.

## Motivation

The existing subagent system can spawn isolated agents, but the patterns are hardcoded in skills. Users and advanced workflows need:
- **Leader/follower**: one agent plans, N workers implement in parallel, leader reviews and merges.
- **Critic loop**: agent writes code → critic reviews → agent revises → repeat until passing.
- **Auto-specialization**: detect that a task needs documentation lookup → spawn a search subagent while the main agent keeps working on edits.
- **Chain of thought decomposition**: break a complex ticket into subtasks, assign each to a specialized subagent, collect results.

## Goals

- Define orchestration patterns as composable YAML/TOML workflows (not Rust code).
- `grok orchestrate plan "implement X"` — decompose and show the plan before executing.
- `grok orchestrate run <workflow-file>` — execute a multi-agent workflow.
- Built-in patterns: leader-follower, critic-loop, decompose-and-conquer, search-then-edit.
- Subagent specialization hints: "this subagent knows Python", "this subagent has web access".
- Progress reporting: show what each subagent is doing in real-time.
- Result aggregation: diff, merge, or vote on subagent outputs.

## Non-Goals

- General-purpose DAG executor (workflows are tree-shaped, not arbitrary DAGs).
- State persistence across workflows (each workflow starts fresh).
- Competitive multi-agent debates (not building an AI debate platform).

## Design

### Architecture

```
┌────────────────────────────────────────────┐
│  xai-grok-shell (main agent)                │
│  User: "Implement this feature"             │
└──────────┬─────────────────────────────────┘
           │
           ▼
┌────────────────────────────────────────────┐
│  xai-grok-orchestrator                      │
│                                            │
│  ┌──────────────────────────────────────┐  │
│  │ Workflow Parser                       │  │
│  │ (YAML → Workflow DAG)                │  │
│  └──────────────────────────────────────┘  │
│  ┌──────────────────────────────────────┐  │
│  │ Pattern Library                      │  │
│  │ - LeaderFollower                     │  │
│  │ - CriticLoop                         │  │
│  │ - DecomposeAndConquer                │  │
│  │ - SearchThenEdit                     │  │
│  └──────────────────────────────────────┘  │
│  ┌──────────────────────────────────────┐  │
│  │ Subagent Pool                         │  │
│  │ (spawn/manage/kill subagents)        │  │
│  └──────────────────────────────────────┘  │
│  ┌──────────────────────────────────────┐  │
│  │ Result Aggregator                    │  │
│  │ (diff, merge, vote, compile)         │  │
│  └──────────────────────────────────────┘  │
└──────────┬─────────────────────────────────┘
           │
           ▼
     ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
     │ Subagent A   │ │ Subagent B   │ │ Subagent C   │
     │ (planner)    │ │ (implementer)│ │ (reviewer)   │
     └─────────────┘ └─────────────┘ └─────────────┘
```

### Workflow YAML Format

```yaml
# leader-follower.yaml
name: "Implement feature with review"
pattern: leader_follower

leader:
  model: grok-3-reasoning
  instructions: |
    Analyze the feature request and create a plan.
    Output a numbered list of files to create/modify.

followers:
  count: 2
  model: grok-3
  instructions: |
    Implement the file changes assigned to you.
    Work independently from other followers.

aggregator:
  strategy: merge  # diff | merge | vote | compile
  review:
    enabled: true
    model: grok-3-reasoning
    instructions: |
      Review all implementations for correctness,
      consistency, and style. Output a unified diff.
```

```yaml
# critic-loop.yaml
name: "Write and refine code"
pattern: critic_loop

actor:
  model: grok-3
  instructions: |
    Implement the requested change. Write clean, tested code.

critic:
  model: grok-3-reasoning
  instructions: |
    Review the implementation critically.
    List specific issues: correctness, edge cases, style, tests.

loop:
  max_iterations: 3
  convergence_criteria: "critic approves with no issues"
```

```yaml
# decompose-and-conquer.yaml
name: "Build full-stack feature"
pattern: decompose_and_conquer

decomposer:
  model: grok-3-reasoning
  instructions: |
    Break this feature into independent subtasks.
  subtasks:
    - id: backend
      description: "Implement API endpoints"
      assign_to: backend-agent
    - id: frontend
      description: "Implement UI components"
      assign_to: frontend-agent
    - id: tests
      description: "Write integration tests"
      assign_to: test-agent

agents:
  backend-agent:
    model: grok-3
    specialization: ["python", "fastapi"]
  frontend-agent:
    model: grok-3
    specialization: ["typescript", "react"]
  test-agent:
    model: grok-3-fast
    specialization: ["pytest", "playwright"]

aggregator:
  strategy: compile  # collect all outputs into one summary
```

### Orchestrator API

```rust
/// The orchestrator trait. Implementations are the built-in patterns.
#[async_trait]
trait OrchestrationPattern: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(
        &self,
        context: OrchestrationContext,
        workflow: WorkflowConfig,
    ) -> Result<OrchestrationResult>;
}

struct OrchestrationContext {
    main_session: SessionHandle,
    workspace: WorkspaceHandle,
    tool_registry: ToolRegistry,
}

struct OrchestrationResult {
    outputs: Vec<SubagentOutput>,
    aggregated: AggregatedResult,
    timeline: Vec<OrchestrationEvent>,
}
```

### CLI Interface

```
grok orchestrate plan "implement login page"        # show decomposition plan
grok orchestrate run ./workflows/leader-follower.yaml  # execute workflow file
grok orchestrate list                                # show available patterns
grok orchestrate status <workflow-id>                # show running workflow progress
grok orchestrate cancel <workflow-id>                # cancel running workflow
```

### TUI Integration

When a workflow is running, the TUI shows a multi-pane view:

```
┌─── Orchestrator ──────────────────────────────┐
│ Workflow: Implement login page                 │
│                                               │
│ ├── ⏳ Planner: analyzing requirements...      │
│ ├── ✅ Backend: API endpoints done             │
│ ├── 🔄 Frontend: building UI (3/5 files)       │
│ └── ⏳ Tests: waiting...                       │
│                                               │
│ [Collapse] [Cancel] [View Logs ▾]             │
└───────────────────────────────────────────────┘
```

## Implementation Plan

### Phase 1 — Pattern Library (3-4 weeks)
1. Create `xai-grok-orchestrator` crate with `OrchestrationPattern` trait.
2. Implement `LeaderFollower` pattern (spawn N subagents, aggregate results).
3. Implement `CriticLoop` pattern (actor → critic → revise loop).
4. Implement `DecomposeAndConquer` pattern (break into subtasks, assign, collect).
5. Tests: each pattern with mock subagents, verify correct execution flow.

### Phase 2 — Workflow Engine (2-3 weeks)
1. Implement YAML workflow parser.
2. Implement subagent pool (spawn/manage/kill with resource limits).
3. Implement progress reporting (real-time status via channels).
4. Implement `grok orchestrate plan` and `grok orchestrate run`.
5. Tests: YAML parsing, subagent pool lifecycle, progress events.

### Phase 3 — TUI Integration (2 weeks)
1. Implement orchestrator status panel in the pager.
2. Add cancel workflow support.
3. Add detailed per-subagent log viewing.
4. Tests: UI rendering with concurrent subagent updates.

## Testing Strategy

| Test Type | What |
|-----------|------|
| Unit | Pattern execution with mock subagents, result aggregation |
| Integration | Full workflow with real subagents → correct output |
| YAML | Parsing valid/invalid workflow files |
| Resilience | Subagent crash → workflow continues or fails gracefully |
| Performance | 10 concurrent subagents → no resource exhaustion |

## Open Questions

1. Should subagents share a sandbox or get isolated sandboxes? *Decision:* Isolated by default. Leader can opt into shared if needed.
2. How to handle conflicting edits from parallel subagents? *Decision:* `merge` strategy uses three-way merge (base + A + B). Conflicts are flagged for manual resolution.
3. Should workflows support conditionals (if subtask A fails, skip subtask B)? *Decision:* Phase 3. MVP uses simple sequential/parallel execution only.
