# Grok Build Gravemod - 10 Issues Hardening Plan

**Context**: 10 documented issues in `.github/issues/*.md` covering security, robustness, and design gaps in:
- HTTP/command hooks (sandboxing, SSRF, shell parsing)
- Sandbox (seccomp, bwrap, profiles, devbox)
- Permissions (bash unparseable, grants)
- Config (signed policy)
- Secrets (JWT redaction)

Many mitigations already exist in the current tree (verified via inspection). Goal: bring all to high confidence with rigorous, multi-turn process.

## Guiding Principles
- Use lean-ctx tools (ctx_compose first for understanding, then targeted ctx_read/patch/shell).
- Small, focused, reviewable diffs.
- Tests first / after every change.
- Threat modeling + red teaming (attempt to exploit).
- 3 full turns of the cycle for depth.
- Track status in the individual .md files + this plan + todos.

## Priority Grouping
**High (start here)**: 01 (DNS rebinding SSRF), 02 (seccomp), 04 (path traversal), 07 (bwrap silent fail)
**Medium**: 03 (backticks), 05 (unparseable bash), 06 (signed policy dark)
**Low**: 08 (profile name collision), 09 (devbox cfg gate), 10 (JWT regex)

## All Phases

### Phase 0: Foundation (Baseline & Inventory)
- [ ] Use lean-ctx to inventory and read all 10 .md files + map to current source locations.
- [ ] Assess current state for each (code already has fix? partial? open?).
- [ ] Update all 10 `.md` files with accurate **Status** (RESOLVED / PARTIALLY / OPEN) + pointers + remaining work.
- [ ] Establish baseline: `cargo check`, relevant test runs (`xai-grok-hooks`, `xai-grok-sandbox`, `xai-grok-config`, `xai-grok-workspace`, `xai-grok-secrets`).
- [ ] Create / update tracking (this PLAN + todo list).
- [ ] Threat model overview for hooks + sandbox + permission system.
- Output: Clean baseline + accurate issue statuses.

### Phase 1: Per-Issue Rigorous Cycle (Core Work)
For each issue (process High first, then Medium, then Low; batch where logical):

Apply the **8-Step Cycle**, then **repeat the full cycle 3 complete turns**.

**The Cycle (one turn)**:
1. **Review**  
   - ctx_compose on relevant crate(s) + issue keywords.  
   - Read the exact functions/files mentioned.  
   - Understand root cause, preconditions, impact, existing tests.  
   - Document simple threat model (attacker goals, entry points).

2. **Testing**  
   - Run all existing tests for the area.  
   - Add or expand unit/integration tests that would have caught the original gap.  
   - Cover edge cases (empty, malformed, race, glob, etc.).

3. **Deep Review**  
   - Architecture: how does this interact with other components (e.g. permission → sandbox → hooks)?  
   - Hunt for similar patterns elsewhere in the codebase (grep + ctx_search).  
   - Alternative designs / defense-in-depth ideas.  
   - Review for new issues introduced by any prior fixes.

4. **Testing (Round 2)**  
   - Run full area tests + any new ones.  
   - Consider property-based or simple fuzz if input is complex (bash, URLs, paths).  
   - Test under different profiles (devbox, strict, off).

5. **Optimization**  
   - If the correct fix adds overhead (extra checks, canonicalization, decoding), measure and optimize.  
   - Prefer safe fast paths, caching where sound.  
   - Never weaken security for speed.

6. **Refactoring**  
   - Improve clarity, naming, extraction of helpers.  
   - Follow project conventions (error handling, tracing, small functions).  
   - Keep diffs small and focused.

7. **Testing (Round 3)**  
   - Full regression after refactor.  
   - Add any tests that the refactor enables or requires.

8. **Attempting to Exploit**  
   - Red-team mindset: try to trigger the original vulnerability or find bypasses.  
   - Manual PoCs, crafted inputs (heredocs, weird paths, DNS tricks, missing bwrap simulation, etc.).  
   - If possible, write a small exploit script or test that demonstrates the attack.  
   - Document what worked / what was blocked and why.  
   - If you find a new bypass, treat it as a new mini-issue and loop.

**Repeat the 8-step cycle 3 full turns** per issue (or per priority group). After each turn, update the issue .md with "Turn N confidence: X/10" + what was done.

### Phase 2: System-Level & Cross-Cutting
- After per-issue work, do system-wide reviews:
  - Sandbox consistency (bwrap vs Seatbelt vs no-enforce).
  - Permission grant model + bash segment evaluation.
  - Hook execution paths (http + command).
  - Config loading + signed policy interaction with permissions.
- End-to-end testing (full TUI flows, subagents, worktrees if relevant).
- Apply the 8-step cycle at system level (at least 1-2 turns).
- Attempt broader exploitation (chained attacks: unparseable bash + path traversal + missing bwrap, etc.).
- Optimization at integration points.
- Refactoring of shared abstractions if patterns emerge.

### Phase 3: Polish, Documentation & Final Hardening
- Update all documentation (user guide sections on sandbox, hooks, permissions if they exist).
- Add or improve high-level tests / harnesses that cover multiple issues.
- Performance pass on hot paths touched (if any).
- Final round of the cycle focused on "what did we miss?"
- Create any follow-up issues or notes for gravemod-specific extensions.

### Phase 4: Verification, Sign-off & Delivery
- Comprehensive test run across affected crates + any integration tests.
- Final "attempt to exploit the entire system" pass (3rd global turn).
- Self deep review + planned code review of all changes.
- Update this PLAN with outcomes and remaining risk.
- (If applicable) Prepare changes for the mod (small PRs or stacked changes).

## Detailed Cycle Definition (the 8 steps above)
(See Phase 1)

**Repeat for 3 turns**:
- Turn 1: Initial fix + basic validation + first exploit attempt.
- Turn 2: Deepening after seeing what the first exploit attempts revealed; more refactoring.
- Turn 3: "Assume we are attackers who know the mitigations" – try harder bypasses, optimization, final polish.

After each full turn for a component: update confidence and the issue .md.

## Tooling & Process Rules
- **Always** start code understanding with `lean-ctx ctx_compose`.
- Prefer `ctx_read` (mode=signatures or anchored for edits) over raw cat.
- Use `ctx_shell` for builds/tests/greps when beneficial.
- Every change: compile + relevant tests pass.
- Small diffs preferred.
- When attempting exploits: prefer adding failing tests that demonstrate the attack (then make them pass after fix).
- Track everything in the individual issue .md files + this plan.

## Current Known Status (as of planning)
(Will be refreshed in Phase 0)
- Many core fixes (DNS pinning, seccomp, bwrap check, conservative blanket for unparseable, etc.) appear present.
- Some areas still benefit from warnings, stricter validation, tests, or docs (e.g. signed policy, JWT).

## Success Criteria
- All 10 .md files have clear "RESOLVED with evidence" or "ACCEPTED RISK + mitigation" + test coverage notes.
- Multiple independent exploit attempts failed after the 3 turns.
- Code is clearer and better tested than when we started.
- No regressions in existing functionality.

---

**Next immediate actions** (see todo list):
1. Execute Phase 0 (inventory + baseline).
2. Prioritize first High issue and begin Turn 1 of the cycle.

This plan will be updated as we execute.
