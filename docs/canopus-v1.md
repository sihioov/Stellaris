# Canopus v1

## Position

Canopus v1 is the first AI development automation workload built on Stellaris. It lives in `apps/canopus/` and should remain portable through ports and adapters. Stellaris provides task dispatch and worker execution; Canopus provides the AI workflow, policy, artifacts, and approval semantics.

## Scope

Canopus v1 handles one request through a fixed stage pipeline:

```text
intake → plan → code/work → check → review → human approval → optional PR/follow-up
```

In scope:

- CLI/Discord-backed development requests.
- `dysonsphere::TaskMessage` integration through a task backend adapter.
- Local branch, diff, test result, review, and run-record artifacts.
- ToolGateway allow/deny policy for git, shell, checks, and future GitHub actions.
- Human approval before risky or final external actions.
- `PendingProposal` handling for Hubble/Kepler-discovered candidates.

Out of scope for v1:

- Multi-agent conversation bus.
- Fully autonomous merge/deploy.
- GitHub Project as the internal source of truth.
- Unbounded retries or unreviewed risky tools.

## Key Modules

```text
apps/canopus/src/core/      # workflow state, agenda, artifacts, errors
apps/canopus/src/ports/     # task backend, agent runtime, tool gateway, artifact store
apps/canopus/src/adapters/  # local/Stellaris implementations
apps/canopus/src/cli/       # command entrypoints
apps/canopus/tests/         # integration and policy tests
```

## Safety Model

Canopus should keep all externally visible or destructive actions behind explicit policy and approval gates. It must not push directly to protected branches, ignore failed checks, overwrite task states unconditionally, or treat discovery findings as approved work.

## Validation

Before changing Canopus behavior, run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Use targeted tests under `apps/canopus/tests/` for ToolGateway policy, artifact persistence, task backend mapping, and workflow transitions.
