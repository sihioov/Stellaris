---
title: Extend Mutation Pipelines with Enum Variants Instead of Bool Params or Sibling Functions
date: 2026-05-10
category: architecture-patterns
module: canopus/cli/finalize
problem_type: architecture_pattern
component: tooling
severity: medium
applies_when:
  - Adding a new privilege level to an existing mutation pipeline (e.g., dry-run → local-commit → push → PR → merge → deploy)
  - The pipeline already has a mode enum gating progressively-riskier steps
  - You are tempted to either fork a sibling function or add a bool parameter to express the new level
tags:
  - canopus
  - finalize-mode
  - pipeline-extension
  - brownfield
  - enum-dispatch
---

# Extend Mutation Pipelines with Enum Variants Instead of Bool Params or Sibling Functions

## Context

Canopus's `post_approval()` async function in `apps/canopus/src/cli/finalize.rs` already executes a staged git mutation: `git add` → `git commit` → `git push` → `gh pr create`. The progression is gated by `FinalizeMode { DryRun, Mutate }` — `DryRun` returns early before any mutation, `Mutate` runs the full sequence. V1.5 introduced an intermediate privilege level: **commit locally but stop before push**, gated by the new env flag `CANOPUS_ALLOW_LOCAL_COMMIT=1`.

Three plausible designs were debated during ralplan consensus:

- **Option A** — Extract a sibling async function `local_commit_only()` parallel to `post_approval()`
- **Option B** — Add a `skip_push: bool` parameter to `post_approval()`
- **Option C (chosen)** — Extend `FinalizeMode` with a third variant `LocalCommitOnly`, branch inside `post_approval()` AFTER commit / BEFORE push

## Guidance

**When extending a mutation pipeline that already has a mode enum, add a new enum variant before considering bool params or sibling functions.** Branch on the variant at the natural privilege boundary inside the existing function. The variant should sit between adjacent privilege levels in the enum to express the monotonic progression (`DryRun < LocalCommitOnly < Mutate`).

```rust
// Before — V1 enum
pub(crate) enum FinalizeMode {
    DryRun,
    Mutate,
}

// After — V1.5 enum (single new variant, between existing two)
pub(crate) enum FinalizeMode {
    DryRun,
    LocalCommitOnly,  // new privilege level
    Mutate,
}

// Inside post_approval(), branch AFTER successful commit, BEFORE push:
async fn post_approval(/* ... */, mode: FinalizeMode) -> Result<String> {
    // dry-run early-return (unchanged)
    if matches!(mode, FinalizeMode::DryRun) {
        return Ok(plan.join("\n"));
    }

    // pre-flight + git add + git commit (shared by LocalCommitOnly and Mutate)
    // ...

    if matches!(mode, FinalizeMode::LocalCommitOnly) {
        return Ok("committed locally, no push".into());
    }

    // git push + gh pr create (Mutate only — unchanged)
}
```

The trigger site (e.g., the watch loop) selects mode via env flag:

```rust
let mode = if env_flag("CANOPUS_ALLOW_LOCAL_COMMIT") {
    FinalizeMode::LocalCommitOnly
} else {
    FinalizeMode::DryRun
};
```

## Why This Matters

**Sibling functions duplicate the audited git plumbing.** A separate `local_commit_only()` would have to repeat the pre-flight checks, `changed_files()` invocation, the "nothing to commit" idempotency carve-out, and the commit-message formatter call. Two copies inevitably drift — when V2 adds a tweak to the commit step, you have to remember to update both. The Architect review explicitly rejected Option A on this ground.

**Bool params lose enum exhaustiveness and obscure intent.** `post_approval(repo, branch, run_id, ..., skip_push: true)` reads as a negation at the call site and is easy to misuse (`skip_push: false` plus `--allow-mutation: false` is ambiguous). It also forecloses on V2 progression: when push and PR creation become independent privileges, you'd add a second bool, then a third. The enum scales linearly with one variant per privilege level.

**Enum variants make the privilege monotonicity explicit.** `DryRun < LocalCommitOnly < Mutate` is readable in the type itself. New `match m { DryRun => ..., LocalCommitOnly => ..., Mutate => ... }` blocks force you to consider every level, which is exactly what you want for security-relevant gating.

**Brownfield-friendly.** The variant addition is purely additive — existing `Mutate` and `DryRun` paths are bit-identical after the change. A successful test run on the existing 73 integration tests + 57 lib tests confirmed no regressions in V1.5.

## When to Apply

- **Adding a new privilege level between existing levels** — e.g., adding "stage but not commit" between `DryRun` and `LocalCommitOnly` in the future, or "push but not PR" between `LocalCommitOnly` and `Mutate`
- **The existing dispatch is `matches!()` or `if/else`** — Architect verified Canopus has only `matches!()` and bool branching, so a new variant doesn't break exhaustive `match` blocks (none exist). Verify this in your codebase first
- **Pure helper extraction is also useful** — V1.5 also extracted three pure functions (`derive_modules`, `derive_branch_name`, `format_commit_message`) for the data-transformation parts of the new variant. Pure helpers + enum dispatch is the synthesis: pure helpers are reusable across modes; mode dispatch keeps effectful code in one place

## When NOT to Apply

- **The existing pipeline does not use a mode enum** — start with one before adding more variants; alternatively, accept that this is a refactor moment and introduce the enum + new variant in the same PR
- **The new mode shares no code with existing modes** — if `LocalCommitOnly` had completely different pre-flight, message generation, and commit logic from `Mutate`, a sibling function would be the right call. The shared code (~80% of the body) is what justifies the variant
- **The enum already has exhaustive `match` blocks scattered across the codebase** — adding a variant requires touching every site. Worth doing, but factor that cost into the design review

## Examples

### Successful application (V1.5 implementation)

The implementation lives at `apps/canopus/src/cli/finalize.rs` (commit `7d0cbec`). Key sites:
- Enum at `finalize.rs:225-235` (3 variants)
- LocalCommitOnly branch inside `post_approval` at `finalize.rs:263-388` (pre-flight + commit + early return before push)
- Watch site mode selection at `finalize.rs:79-89` (env flag dispatch)
- Mutate path at `finalize.rs:404-446` (unchanged from V1)

### Anti-pattern that ralplan caught

The Planner's iteration-1 plan proposed Option A (sibling function `local_commit_only()`). The Architect's steelman antithesis surfaced that:

1. Two copies of `changed_files()` parsing would drift
2. The "nothing to commit" idempotency carve-out (existing `:259-264`) would have to be manually replicated
3. `post_approval(DryRun)` and `local_commit_only()` would race in the watch loop (both need the same gate logic)

Iteration-2 adopted the enum variant synthesis. The lesson: when you're tempted to fork a function to add a privilege level, check whether the new path shares ≥ 50% of the body with the existing function. If yes, a variant is almost always better.

## Related

- `docs/solutions/runtime-errors/codex-context-overflow-prior-artifact-2026-05-09.md` — separate V1 fix for unrelated runtime issue (no overlap)
- `docs/solutions/integration-issues/canopus-dirty-worktree-on-submit-gitignore-2026-05-09.md` — `.canopus/` gitignore prerequisite is also referenced by V1.5 LocalCommitOnly pre-flight
- `.omc/specs/deep-dive-canopus-approve-auto-commit.md` — full V1.5 spec
- `.omc/plans/ralplan-canopus-approve-auto-commit-2026-05-09.md` — ralplan consensus plan with ADR
