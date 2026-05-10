---
title: "Approval finalization should not require watch daemon"
date: 2026-05-10
category: logic-errors
module: canopus/europa
problem_type: logic_error
component: development_workflow
symptoms:
  - "Discord `!approve` moved a task to `Processed`, but no local commit was created"
  - "Operators had to infer whether `canopus watch` was running before approval could finalize"
  - "DryRun sidecars could be confused with completed local finalization evidence"
root_cause: missing_workflow_step
resolution_type: workflow_improvement
severity: high
related_components:
  - canopus-cli
  - europa-discord-bot
  - finalize-mode
  - local-commit
  - task-approval
  - watch-loop
tags:
  - canopus
  - europa
  - approval
  - finalize-approved
  - watch
  - local-commit
  - dry-run
  - discord
---

# Approval finalization should not require watch daemon

## Problem

Discord approval was treated as a state transition that a separate `canopus watch` loop would later discover. When `!approve discord-8c04ecc8c056` moved the task to `Processed` but no watcher was running, no local commit happened, even though operators expected approval to complete the gated local finalization path.

The fix in commit `c8d5ccc` made approval event-triggered: Europa persists approval, then invokes a bounded Canopus finalizer for that exact task.

## Symptoms

- `!approve <task_id>` completed, but no commit appeared on the task branch.
- The task had approval evidence, but no finalization record or local commit was produced until a watch loop ran.
- Operators had to reason about whether Canopus should be a service, daemon, `watch --once` command, or one-shot CLI.
- Re-running finalization needed a safe operator UX instead of manually starting a broad watcher.

## What Didn't Work

- Relying on `canopus watch` after approval. Session history showed TON618, Laniakea, and Europa running, but no `canopus watch`; approval moved the task to `Processed` without producing finalization output (session history).
- Treating approval as only a background scheduling hint. This left operators unable to distinguish “not approved,” “watcher not running,” “gate disabled,” and “finalization failed.”
- Assuming the V1.5 `LocalCommitOnly` mode was enough. Commit `7d0cbec` added the gated local-commit behavior, but the approval path still needed an explicit invocation point (session history).
- Letting DryRun sidecars imply terminal completion. DryRun evidence is observational only; it must not block later gate-on local commit.

## Solution

Add a single-shot Canopus finalizer and make Discord approval call it directly.

```bash
canopus finalize-approved \
  --tasks <tasks.json> \
  --task-id <task_id> \
  --json
```

Europa now follows this sequence:

1. Persist approval first: `Processed`, `approval_state=approved`, `finalize_requested_at`, and Discord provenance.
2. Invoke `finalize_approved_task(tasks_path, task_id)`.
3. Keep approval intact if finalization fails.
4. Tell the operator to retry with `!finalize <task_id>`.

The retry command is intentionally narrow:

```text
!finalize <task_id>
```

Canopus owns all policy and mutation decisions. `finalize-approved` selects exactly one approved `Processed` task, validates approval/finalize evidence, derives the same run identity as submit, and emits structured JSON for success or failure.

Local commit remains explicitly gated:

```bash
CANOPUS_ALLOW_LOCAL_COMMIT=1 \
canopus finalize-approved \
  --tasks /path/to/tasks.json \
  --task-id discord-8c04ecc8c056 \
  --json
```

In `LocalCommitOnly`, finalization commits only on the existing expected task branch. It must not create or check out branches, push, open a PR, merge, deploy, or manage daemon lifecycle.

Pre-commit safety gates fail before `git add` or `git commit` when the repo is unsafe:

- detached `HEAD`
- protected branch such as `main`, `master`, or `develop`
- current branch does not match the expected task branch
- dirty index
- `.canopus/` is not gitignored

Status semantics are operator-visible and should stay precise:

- `dry_run`: gate disabled; sidecar is observational only.
- `finalized`: local commit created.
- `already_finalized`: prior local finalization is terminal.
- `no_changes`: approved task had nothing new to commit.

A repeated DryRun returns `dry_run` with `idempotent=true`; it must not return `already_finalized`. A prior DryRun sidecar can later be upgraded by a gate-on `LocalCommitOnly` run.

## Why This Works

Approval is the human decision point, so finalization must be adjacent to the approval event rather than discovered later by a background poller. The one-shot command removes the hidden daemon dependency while preserving the repository boundary:

- Europa owns Discord UX, approval persistence, and retry messaging.
- Canopus owns task eligibility, run identity, git policy, sidecars, and structured JSON responses.
- The operator sees immediate success/failure feedback after `!approve`.
- Retrying uses a task-scoped command instead of a broad watch sweep.
- DryRun evidence remains safe to produce without blocking a later gated commit.

The solution also preserves the LocalCommitOnly privilege model documented in `docs/solutions/architecture-patterns/finalize-mode-enum-extension-2026-05-10.md`: add behavior through the existing finalization mode pipeline, but do not widen the approval path into push/PR/merge/deploy.

## Prevention

- Treat approval and finalization as adjacent but distinct steps: persist approval first, then attempt finalization once.
- Keep retry idempotent and task-scoped with `!finalize <task_id>`.
- Do not make operator-facing approval depend on an unobserved background watcher.
- Keep all git mutation policy in Canopus, not the Discord surface.
- Never let DryRun sidecars masquerade as completed local commits.
- Add explicit JSON tests for every operator-visible status and failure code.
- Test both direct `finalize-approved` and `watch --once` idempotency paths when changing finalization sidecar semantics.

Regression coverage added with the fix:

- `apps/canopus/tests/finalize_approved.rs` covers dry-run, repeated dry-run, local commit, repeated local success, no-changes, missing/duplicate task IDs, eligibility failures, branch preflight failures, and DryRun-to-LocalCommitOnly upgrade.
- `apps/canopus/tests/auto_commit_gate_off.rs` covers `watch --once` upgrading a prior DryRun sidecar to a gated local commit.
- `apps/europa/test_bot_config.py` covers `!approve`, `!finalize`, finalize command construction, and nonzero `ok:false` JSON parsing.

Validation run for `c8d5ccc`:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 -m py_compile apps/europa/europa.py apps/europa/canopus_client.py
python3 -m unittest apps/europa/test_bot_config.py
```

## Related Issues

- `docs/solutions/architecture-patterns/finalize-mode-enum-extension-2026-05-10.md` — related mode-extension pattern for `DryRun -> LocalCommitOnly -> Mutate`.
- `docs/solutions/integration-issues/canopus-dirty-worktree-on-submit-gitignore-2026-05-09.md` — related `.canopus/` gitignore precondition for safe local commit workflows.
- `apps/canopus/src/cli/finalize.rs` — `finalize-approved`, sidecar idempotency, and current-branch-only finalization policy.
- `apps/europa/europa.py` — Discord `!approve` and `!finalize` operator UX.
