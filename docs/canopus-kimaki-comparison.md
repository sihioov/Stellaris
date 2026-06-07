# Canopus vs Kimaki Comparison

## Summary

Kimaki and Stellaris Canopus overlap around AI coding orchestration, Discord control, sessions, worktrees, permissions, and agent runtime execution. They are not the same kind of system.

- **Kimaki** is a Discord-first OpenCode control plane: Discord channels map to projects, Discord threads map to coding sessions, and users drive OpenCode from Discord.
- **Stellaris Canopus** is a Stellaris-internal execution engine: it owns task policy, workflow stages, artifacts, approval/finalization gates, and runner orchestration behind the Europa Discord surface.

In short: Kimaki is closer to a polished Discord UX/runtime for OpenCode sessions. Canopus is closer to a policy-owned job execution engine inside a larger distributed task system.

## High-Level Difference

| Area | Stellaris Canopus | Kimaki |
|---|---|---|
| Primary role | AI development execution engine inside Stellaris | Collaborative agent orchestrator inside Discord |
| Main UX model | Europa creates/approves tasks; Canopus executes policy-owned runs | Discord channel = project, Discord thread = OpenCode session |
| Main execution unit | agenda/task/run | thread/session |
| Runtime style | staged pipeline: intake, plan, work, check, review, approval/finalize | interactive OpenCode sessions with queue, interrupt, resume, fork |
| State model | per-project `.canopus/` artifacts/run records plus Stellaris task status | SQLite database mapping channels, threads, sessions, worktrees, models, agents, schedules |
| Surface boundary | Europa is Discord UI; Canopus must stay surface-agnostic engine | Kimaki combines Discord UX and OpenCode session orchestration in one product |
| Mutation policy | Canopus owns GitHub/project/live mutation gates | Kimaki owns OpenCode session permissions and user-facing command controls |
| GitHub direction | PR/CI/review/merge/deploy gates are part of the long-term delivery system | Diff/share/worktree/session workflow is primary; GitHub delivery is not the same core contract |

## Stellaris Canopus

Canopus is positioned as the first AI development automation workload built on Stellaris. Stellaris provides task dispatch and worker execution; Canopus provides the AI workflow, policy, artifacts, and approval semantics.

The intended v1 pipeline is:

```text
intake -> plan -> code/work -> check -> review -> human approval -> optional PR/follow-up
```

The current architecture boundary is explicit:

- **Europa** is the Discord UI adapter. It should translate commands and responses but not own policy.
- **Canopus** owns app policy, state mutation decisions, workflow semantics, GitHub/project gates, approval/finalization behavior, and durable artifacts.
- **Dysonsphere** owns the shared task/status contract used by producers, schedulers, workers, and app workloads.

Important Canopus code structures:

- `apps/canopus/src/core/pipeline.rs` maps task types into role pipelines such as `DevMode`, `BugFix`, `SecurityAudit`, `TestWriter`, and `UXImprovement`.
- `apps/canopus/src/core/workflow.rs` defines the workflow state machine: `Created -> Planned -> Executing -> Checking -> Reviewed -> Completed`, with `Failed` allowed from any non-failed state.
- `apps/canopus/src/cli/submit.rs` is the main orchestration path. It resolves runtime backend, checks the worktree, runs role stages, persists artifacts, and records workflow transitions.
- `apps/canopus/src/core/runtime_registry.rs` selects backend capability and preparation policy from role mode, task type, backend directives, and env configuration.
- `apps/canopus/src/ports/` defines the hexagonal contracts: `AgentRuntime`, `ArtifactStore`, `TaskBackend`, and `ToolGateway`.

Canopus is therefore best understood as a **policy and execution kernel** for AI development jobs.

## Kimaki

Kimaki is a TypeScript/pnpm monorepo, but its core product is the `cli/` package: a long-running Discord bot and OpenCode orchestration layer.

Kimaki's central model is:

```text
Discord channel = project
Discord thread = coding session
```

A first message in a project channel creates a Discord thread and starts an OpenCode session. Messages inside the thread continue that same session. This gives each piece of work its own conversation, history, and resumable context.

Important Kimaki code structures:

- `cli/src/cli.ts` is the main CLI entrypoint for setup, options, and command registration.
- `cli/src/discord-bot.ts` is the long-lived Discord event loop.
- `cli/src/opencode.ts` manages one shared `opencode serve` process and scopes clients per project directory.
- `cli/src/session-handler/thread-session-runtime.ts` is the per-thread runtime. It owns listener handles, typing timers, event buffers, serialized action queues, ingress preprocessing, queue draining, abort handling, and OpenCode session creation/reuse.
- `cli/src/schema.ts` and `cli/src/database.ts` store thread/session mappings, channel directories, worktrees, models, agents, scheduled tasks, and session event snapshots in SQLite.
- `cli/src/commands/new-worktree.ts` and `cli/src/commands/merge-worktree.ts` implement the user-facing worktree flow.

Kimaki is therefore best understood as a **Discord-native OpenCode session control plane**.

## Overlap

Canopus and Kimaki overlap in these areas:

1. **Discord as control surface**
   - Kimaki makes Discord the primary product surface.
   - Stellaris uses Europa as the Discord surface, but keeps policy in Canopus.

2. **Project/session separation**
   - Kimaki uses channel/project and thread/session as the core UX.
   - Stellaris direction documents also move toward task-specific Discord threads as job sessions.

3. **Worktree isolation**
   - Kimaki lets users move a thread into a worktree and later merge it back.
   - Canopus has `worktree create` support, but it is tied to policy-owned job execution.

4. **Agent runtime invocation**
   - Kimaki drives OpenCode sessions through the OpenCode SDK/server.
   - Canopus invokes configurable runtimes such as mock, command, Codex CLI, or plugin command backends.

5. **Permissions and safety**
   - Kimaki manages OpenCode permission rules and per-session directory access.
   - Canopus owns broader mutation gates: GitHub, Project v2, PR, merge, deploy, local commit, and approval/finalization.

## What Canopus Can Learn From Kimaki

### 1. Thread as job/session UX

Kimaki's clearest strength is the mental model: one Discord thread per session. Stellaris already points in this direction for task threads. The useful lesson is to make job/thread mapping first-class and visible:

```text
discord_thread_id -> job_id -> Canopus workspace -> branch/worktree -> PR/CI state
```

### 2. Interactive message handling

Kimaki distinguishes between:

- normal messages, which can interrupt an active OpenCode run after a timeout;
- queued messages, which wait until the current run finishes;
- side-channel messages such as `btw`, which fork context without disturbing the main run.

Canopus currently behaves more like a staged job pipeline. If Stellaris wants richer operator interaction inside a task thread, Kimaki's interrupt/queue/fork model is a useful reference.

### 3. Worktree UX

Kimaki treats worktrees as a session-level affordance: create, work, rebase/merge, and ask the agent to resolve conflicts. Canopus already has policy-owned worktree creation, but Kimaki is a better reference for user-facing ergonomics.

### 4. Queryable session state

Kimaki's SQLite schema makes channels, threads, sessions, worktrees, models, agents, schedules, and event snapshots directly queryable. Canopus persists artifacts and run records, but its long-term job/session model could benefit from similarly explicit records for runner session IDs, checkpoints, branches, PRs, and CI state.

## What Canopus Should Not Copy Directly

Canopus should not become Kimaki wholesale.

Kimaki intentionally combines Discord UX, OpenCode session control, permissions, queueing, worktrees, scheduling, and setup into one product. Stellaris has a stricter layered boundary:

```text
Europa = Discord UI surface
Canopus = execution engine + policy owner + runner orchestrator
Dysonsphere = shared task/status contract
TON618/Laniakea = scheduling and worker dispatch
```

If Kimaki-like behavior is added to Stellaris, the boundary should stay intact:

- Discord formatting, slash/command UX, and thread presentation belong in Europa.
- Job lifecycle, mutation policy, approvals, artifacts, runner selection, and finalization belong in Canopus.
- Shared task/status schema belongs in Dysonsphere.

## Recommended Direction

Use Kimaki as a UX and runtime-reference project, not as a replacement architecture.

Good candidates to adapt:

1. Make task threads a first-class job-session primitive.
2. Add explicit follow-up semantics inside a job thread.
3. Separate immediate interrupt, queued follow-up, and side-question/fork behavior.
4. Improve worktree lifecycle UX while keeping Canopus as policy owner.
5. Add a queryable job/session ledger that records runner session ID, checkpoint artifacts, branch/worktree, PR, and CI state.

Avoid copying:

1. Surface-specific Discord formatting into Canopus.
2. OpenCode-specific assumptions into the Canopus core engine.
3. A monolithic Discord runtime that bypasses Stellaris task contracts.
4. Direct mutation decisions from Europa.

## Evidence

Stellaris / Canopus:

- `docs/canopus-v1.md` - Canopus v1 scope, safety model, runtime modes, and module map.
- `docs/architecture/boundaries.md` - authoritative Europa / Canopus / Dysonsphere boundary.
- `docs/stellaris-canopus-direction-summary.md` - long-term direction for Discord-issued development work, Canopus runner orchestration, GitHub PR/CI/review flow, and human-in-the-loop gates.
- `apps/canopus/src/cli/submit.rs` - main Canopus execution loop.
- `apps/canopus/src/core/pipeline.rs` - task-type to role-pipeline mapping.
- `apps/canopus/src/core/workflow.rs` - workflow state machine.
- `apps/canopus/src/core/runtime_registry.rs` - runtime/backend selection.
- `apps/canopus/src/ports/` - port contracts for runtime, artifacts, task backend, and tool gateway.
- `apps/europa/README.md` - Discord surface commands and Canopus finalization boundary.
- `laniakea/src/handlers/custom.rs` - production path that invokes `canopus submit`.

Kimaki:

- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/README.md` - channel/project and thread/session model.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/website/src/docs/docs/core-concepts/channels-threads.mdx` - detailed project/session mapping.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/website/src/docs/docs/core-concepts/message-handling.mdx` - interrupt behavior.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/website/src/docs/docs/features/queue.mdx` - local queue behavior.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/website/src/docs/docs/features/worktrees.mdx` - worktree UX.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/cli/src/cli.ts` - CLI entrypoint.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/cli/src/opencode.ts` - shared OpenCode server and per-directory clients.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/cli/src/session-handler/thread-session-runtime.ts` - per-thread runtime.
- `/home/sihio/.kimaki/projects/kimaki-clone/kimaki/cli/src/schema.ts` - SQLite schema for sessions, channels, worktrees, models, agents, schedules, and event snapshots.
