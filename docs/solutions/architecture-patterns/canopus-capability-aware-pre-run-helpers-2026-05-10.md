---
title: Canopus Capability-Aware Agents Use Policy-Owned Read-Only Pre-Run Helpers
date: 2026-05-10
category: architecture-patterns
module: apps/canopus
problem_type: architecture_pattern
component: assistant
severity: medium
applies_when:
  - "Adding role-specific helper agents that must be explicitly enabled before planner, coder, or reviewer execution"
  - "Separating Codex prompt roles from Canopus helper orchestration and capability policy"
  - "Running repository exploration helpers with read-only guarantees and provenance artifacts"
  - "Introducing environment-gated helper backends such as off, mock, or repo-explore"
related_components:
  - development_workflow
  - tooling
  - documentation
  - testing_framework
tags:
  - canopus
  - role-agents
  - pre-run-helpers
  - repo-explore
  - read-only-guard
  - provenance
  - codex-prompts
---

# Canopus Capability-Aware Agents Use Policy-Owned Read-Only Pre-Run Helpers

## Context

Canopus needed a way to make planner, coder, and reviewer roles more capability-aware without turning Discord prompts into plugin commands or adding a long-running service. The concrete V1 need was repository-local context before role execution, starting with an `omx explore`-backed helper, while preserving the existing one-shot `canopus submit` pipeline.

The accepted pattern is an explicitly enabled pre-run helper layer:

- Canopus selects helpers from core policy, not from raw user or Discord text.
- Helpers run before eligible role runtime execution.
- Helpers are read-only context providers, not mutation agents.
- Every helper attempt writes durable provenance.
- Only successful helper artifacts are attached to role context.
- Codex prompts label helper context separately from ordinary prior artifacts.

Session history shows this decision followed a rejected mid-run capability-request model where role agents would return `capability_request`, Canopus would run a helper, then re-invoke the role. That model was set aside for V1 because it would expand role re-entry, loop control, timeout handling, fallback semantics, and workflow state transitions (session history).

## Guidance

Model helper execution as a small Canopus domain seam plus an adapter-owned backend port. Keep the runtime contract unchanged for V1; feed successful helper output through existing artifact context.

Core vocabulary belongs in `apps/canopus/src/core/pre_run_helper.rs`:

```rust
pub enum PreRunHelperMode {
    Off,
    RepoExplore,
    Mock,
}

pub enum PreRunHelperFailurePolicy {
    Advisory,
    FailFast,
}

pub struct PreRunHelperConfig {
    pub mode: PreRunHelperMode,
    pub max_output_bytes: usize,
    pub failure_policy: PreRunHelperFailurePolicy,
}
```

Use env-only rollout switches for V1:

```bash
# default/off: no helper execution
CANOPUS_PRE_RUN_HELPERS=off

# deterministic integration tests
CANOPUS_PRE_RUN_HELPERS=mock

# real repository lookup helper
CANOPUS_PRE_RUN_HELPERS=repo-explore

# optional bounds and failure behavior
CANOPUS_PRE_RUN_HELPER_MAX_OUTPUT_BYTES=6000
CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY=advisory
CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY=fail-fast
```

The backend port should be read-only by contract:

```rust
pub trait PreRunHelperBackend {
    fn identity(&self) -> String;

    fn run(
        &self,
        repo: &Path,
        request: &HelperRequest,
        selection: &HelperSelection,
    ) -> CanopusResult<HelperOutput>;
}
```

For `repo-explore`, construct argv directly and avoid shell strings:

```text
omx explore --prompt <derived prompt>
```

The adapter must enforce read-only behavior, not merely document it:

- Allowlist the executable; V1 allows only `omx`.
- Isolate helper state/cache/log/runtime directories outside the target repo.
- Bound timeout and output size.
- Capture pre/post mutation snapshots.
- Include `git status --porcelain=v1 --ignored`.
- Watch ignored write-prone paths such as `.omx/`, `.canopus/`, and `target/`.
- Treat mutations and nonzero exits as helper failures.
- In default advisory mode, continue role execution without attaching failed helper output.

`apps/canopus/src/cli/submit.rs` is the V1 integration seam. Run helpers after task backend submission and before `runtime.run(...)`:

1. Select helpers from `PreRunHelperConfig`, role, and stage.
2. Generate deterministic helper artifact task IDs.
3. Run the backend.
4. Persist `ArtifactKind::HelperProvenance` for every selected helper attempt.
5. Append successful helper artifacts to `prior_artifacts`.
6. Persist a `helper:<stage>` stage record.
7. Fail only when the helper is required or `fail-fast` is configured.

Provenance paths should be predictable:

```text
.canopus/artifacts/<role-task-id>-helper-<helper-name>-<ordinal>/helper-provenance.md
```

Codex prompt rendering should keep helper context visibly separate:

```text
Pre-run helper context (Canopus-selected, read-only):
...
Prior artifacts:
...
```

## Why This Matters

This pattern gives role agents better repository context without weakening Canopus's safety boundaries.

It preserves three separations:

1. **Selection boundary** — Canopus chooses helper capabilities; Discord and user prompts do not.
2. **Mutation boundary** — helpers inspect and summarize; mutation belongs to explicit Canopus workflow stages.
3. **Runtime boundary** — helper output flows through artifacts, so V1 does not require an `AgentRuntime` trait redesign.

The durable provenance artifact also makes helper behavior auditable. Operators can inspect which helper ran, why it was selected, what backend identity was used, whether the read-only guard passed, and whether the output was attached.

Session history adds one caution: earlier Canopus mutation/provenance work repeatedly broke smoke or integration fixtures when new evidence requirements were not modeled in tests. Treat helper provenance and mutation guards as test-contract changes, not incidental logging (session history).

## When to Apply

Use this pattern when all of these are true:

- A role would benefit from repository-local context before execution.
- Helper selection should be product policy, not user-invoked plugin syntax.
- The helper can be read-only.
- Operators need a reversible env-gated rollout.
- Helper failure should usually be advisory.
- Existing runtime prompts can consume helper output as artifacts.
- Provenance is required for debugging or operator trust.

Do not use this pattern when:

- The capability needs to write code, update state, push, open PRs, or mutate external systems.
- The capability must be invoked dynamically by a role agent mid-run.
- A long-running service or daemon is required.
- Helper failure must always block execution without explicit operator opt-in.
- The helper context is so structured that the runtime trait must change; that is a future design, not V1.

## Examples

Enable deterministic helper coverage in tests:

```bash
CANOPUS_PRE_RUN_HELPERS=mock cargo test -p canopus --test cli_submit
```

Enable real repository exploration in an operator environment:

```bash
CANOPUS_PRE_RUN_HELPERS=repo-explore
CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY=advisory
```

Require helper success before roles run:

```bash
CANOPUS_PRE_RUN_HELPERS=repo-explore
CANOPUS_PRE_RUN_HELPER_FAILURE_POLICY=fail-fast
```

Expected advisory failure behavior:

```text
repo-explore mutates .omx/, .canopus/, target/, tracked files, or untracked files
→ read-only guard fails
→ helper-provenance.md records status: failed
→ helper output is not attached
→ role execution continues in advisory mode
```

Representative regression anchors:

- selector and path-safe helper artifact IDs in `apps/canopus/src/core/pre_run_helper.rs`
- helper artifact filename in `apps/canopus/tests/local_file_artifact_store.rs`
- mock helper attachment and failed `repo-explore` mutation behavior in `apps/canopus/tests/cli_submit.rs`
- Codex prompt section separation in `apps/canopus/tests/codex_agent_runtime.rs`
- UTF-8-safe output truncation in `apps/canopus/src/adapters/pre_run_helper/repo_explore.rs`

## Related

- `docs/solutions/architecture-patterns/finalize-mode-enum-extension-2026-05-10.md` — related pattern for extending privileged Canopus flows by adding explicit enum states rather than bools or sibling functions.
- `docs/solutions/logic-errors/approval-finalize-without-watch-daemon-2026-05-10.md` — related boundary lesson: event-triggered Canopus policy should not depend on an unobserved background watcher.
- `docs/solutions/runtime-errors/codex-context-overflow-prior-artifact-2026-05-09.md` — related artifact-context lesson for Codex prompt size and prior artifact hygiene.
- `docs/solutions/integration-issues/canopus-dirty-worktree-on-submit-gitignore-2026-05-09.md` — related state-directory hygiene for `.canopus/` and clean-worktree assumptions.
- `.omx/plans/canopus-capability-aware-agents.md` — implementation plan that captured the V1 pre-run helper decision.

Verification for the implementation that produced this learning passed with:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
git diff --check
```
