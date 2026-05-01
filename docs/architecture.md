# Stellaris Architecture

## Direction

Stellaris is a general-purpose distributed task processing platform. Canopus is an AI development automation app that runs on top of Stellaris; it is not a replacement for the core pipeline.

```text
Producer → Task Contract / Queue → Scheduler → Worker → App / Processing → Result
```

## Core Components

| Component | Layer | Responsibility |
|---|---|---|
| Hubble | Producer / collector | Collects external signals, data, and candidate work from APIs, web, RSS, SNS, issues, or webhooks. |
| Dysonsphere | Shared contract | Defines `TaskMessage`, `TaskStatus`, task storage, discovery helpers, and queue abstractions. |
| TON618 | Scheduler / dispatcher | Selects eligible `Pending` tasks and dispatches them to workers. |
| Laniakea | Worker / executor | Executes tasks by type and records status/results. |
| Canopus | App workload | Runs AI development workflows, tool policy, artifacts, approvals, and git/PR orchestration. |
| Kepler | Discovery source | Finds internal codebase signals such as clippy warnings, test failures, coverage gaps, and security findings. |

## Runtime Flow

```text
Discord / CLI / GitHub / Discovery source
→ TaskMessage or PendingProposal
→ human approval when required
→ Pending task
→ TON618 dispatch
→ Laniakea execution
→ optional app workload such as Canopus
→ artifact / status / result persistence
```

Discovery sources never create directly executable work. Hubble and Kepler register candidates as `PendingProposal`; a human must promote them to `Pending` before TON618 can dispatch them.

## Responsibility Boundaries

Stellaris Core owns task contracts, queues, scheduling, worker execution, safe state transitions, retries, timeouts, and general observability. Canopus owns AI workflow semantics: agent stages, tool policy, artifact models, approval gates, git branches, PRs, and AI runtime adapters.

Avoid these boundary violations:

- Core crates depending on Canopus-specific workflow states or artifact schemas.
- Canopus replacing TON618 scheduling or Laniakea worker execution.
- Hubble/Kepler bypassing `PendingProposal` approval.
- Discord messages, local ledgers, or cache files becoming the source of truth.
- Workers overwriting cancelled, rejected, completed, or reviewed states unconditionally.
