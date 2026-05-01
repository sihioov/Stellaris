# Canopus Design

## Summary

Canopus is a portable AI development orchestration layer. It starts inside the Stellaris monorepo as `apps/canopus`, but its core design must remain backend-agnostic so it can later move to its own repository or run on other task backends.

Stellaris remains a general distributed task processing engine. Canopus is the first AI development workload built on top of it, not a replacement for the existing Stellaris direction.

The first milestone is a Local Patch MVP:

1. Accept a development request from a CLI command.
2. Create an orchestration task and plan artifact.
3. Dispatch agent work through a backend adapter.
4. Create a local git branch and patch.
5. Run local checks.
6. Save diff, test, and review artifacts.
7. Report the result to the user without pushing, creating PRs, merging, or deploying.

## Architecture

Canopus will use a ports-and-adapters design.

```text
apps/canopus/
├── core/
├── ports/
├── adapters/
└── cli/
```

`core` contains orchestration-domain concepts only:

- `Agenda`: the user-facing development request.
- `Plan`: the proposed execution plan.
- `AgentTask`: a bounded unit assigned to an AI worker role.
- `AgentRun`: one execution attempt by an agent runtime.
- `Artifact`: saved outputs such as plans, diffs, test results, and reviews.
- `Approval`: human gate state for risky or final actions.
- `WorkflowState`: the lifecycle of a Canopus task.

`ports` define external dependencies:

- `TaskBackend`: submits and tracks work on Stellaris or another backend.
- `AgentRuntime`: runs planner, coder, reviewer, or future agent roles.
- `ToolGateway`: exposes controlled git, shell, test, lint, and future PR operations.
- `ArtifactStore`: persists plans, logs, diffs, reviews, and test results.
- `ApprovalStore`: records required human decisions.
- `Intake`: accepts requests from CLI first, Discord/GitHub later.

`adapters` implement those ports. Canopus core must not directly depend on Stellaris, Codex, Claude, GitHub, Discord, or local shell details.

## MVP Behavior

Initial adapters:

- `TaskBackend`: Stellaris adapter using `dysonsphere` task contracts and `ton618` scheduling/dispatch concepts.
- `AgentRuntime`: adapter boundary for Codex CLI first, with a mock runtime available for dry-run validation.
- `ToolGateway`: local git and shell adapter with explicit command allowlisting.
- `ArtifactStore`: local filesystem store.
- `ApprovalStore`: local/manual approval records.
- `Intake`: CLI commands such as `canopus submit`, `canopus status`, and `canopus artifacts`.

The MVP intentionally excludes Discord intake, GitHub issue intake, remote push, PR creation, auto-merge, production deployment, web UI, and fully autonomous issue discovery. These are later adapters or workflows after the local patch loop is proven.

The local patch workflow:

```text
canopus submit "<request>"
→ create Agenda
→ create Plan artifact
→ split into AgentTask records
→ submit through TaskBackend
→ run selected AgentRuntime
→ use ToolGateway to create a local branch and patch
→ run configured checks
→ save diff/test/review artifacts
→ print result summary
```

## Extension Path

Canopus should support future autonomous development behavior without making it part of the first milestone:

- Repo scanner agents can later propose issues or improvements.
- Planner, coder, reviewer, critic, security, docs, and DevOps agents can coordinate through `AgentTask` and `Artifact`.
- Discord, GitHub Issues, GitLab Issues, and web UI can become additional `Intake` adapters.
- GitHub/GitLab PR creation can become a `ToolGateway` capability behind approval policy.
- Kubernetes jobs, GitHub Actions, or another queue can become additional `TaskBackend` adapters.

The design rule is: build portable interfaces now, but implement only the Stellaris/local adapters needed for the first working loop.

## Safety And Policy

Canopus must keep destructive or externally visible actions behind explicit policy gates.

Initial hard limits:

- No direct push to protected branches.
- No automatic merge.
- No production deployment.
- No secret or environment-file modification without approval.
- No destructive shell commands by default.
- No PR creation in the Local Patch MVP.

The first version can create local branches, edit local files through the agent runtime, run tests/checks, and save artifacts. Anything beyond local patch generation should be modeled as a future approved capability.

## Test And Acceptance Criteria

The design is accepted when the Local Patch MVP can demonstrate:

- A CLI request becomes a persisted Canopus task.
- A plan artifact is created before code execution.
- At least one agent runtime adapter can execute a bounded task.
- A local branch and diff are produced.
- Checks run and their output is captured.
- Diff, test, and review artifacts are stored.
- Canopus can report task status and artifact paths.
- No remote push, PR creation, merge, or deploy occurs.

Implementation tests should cover:

- Core state transitions.
- Port contract behavior using mock adapters.
- Stellaris task backend mapping from `AgentTask` to existing task contracts.
- Local artifact persistence.
- CLI submit/status/artifacts flows.
- Policy rejection for disallowed external or destructive actions.

## Assumptions

- Canopus starts inside the Stellaris monorepo at `apps/canopus`.
- The name `Canopus` is fixed for the orchestration layer.
- The first useful milestone is Local Patch MVP, not plan-only and not PR automation.
- Stellaris remains a general distributed task processing system.
- Canopus core should be portable enough to split into a separate repository later.
- Only one production backend adapter is required initially: Stellaris.
- A mock agent runtime is allowed for validation, but the intended real runtime adapter starts with Codex CLI.
