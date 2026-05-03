# Canopus v1

## Position

Canopus v1 is the first AI development automation workload built on Stellaris. It lives in `apps/canopus/` and should remain portable through ports and adapters. Stellaris provides task dispatch and worker execution; Canopus provides the AI workflow, policy, artifacts, and approval semantics.

## Scope

Canopus v1 handles one request through a fixed stage pipeline:

```text
intake → plan → code/work → check → review → human approval → optional PR/follow-up
```

In scope:

- CLI/Discord-backed development requests that carry GitHub Issue/Project agenda metadata.
- `dysonsphere::TaskMessage` integration through a task backend adapter.
- Local branch, diff, test result, review, and run-record artifacts.
- TON618/Laniakea metadata handoff: Laniakea invokes `canopus submit --agenda-id <task_id> --task-type <task_type>` so Canopus can select the matching role pipeline while preserving the upstream task id in backend payloads and run records.
- ToolGateway allow/deny policy for git, shell, checks, and future GitHub actions.
- Human approval before risky or final external actions, with Discord approval/rejection recorded in task payloads before Canopus finalization.
- `PendingProposal` handling for Hubble/Kepler-discovered candidates.

Out of scope for v1:

- Multi-agent conversation bus.
- Fully autonomous merge/deploy.
- Treating Discord or local task files as the long-term agenda source of truth; GitHub Issue/Project metadata is in v1 scope while the file queue remains the local transport.
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

Canopus should keep all externally visible or destructive actions behind explicit policy and approval gates. It must not push directly to protected branches, ignore failed checks, overwrite task states unconditionally, or treat discovery findings as approved work. `submit` does not create GitHub Q&A issues unless `CANOPUS_ENABLE_GITHUB=1` is set; local pipeline launchers keep this disabled for dry-run safety.

### Dry-run and live gaps

- Local validation uses the deterministic `MockAgentRuntime` and file-backed task tables.
- GitHub issue creation is disabled unless `CANOPUS_ENABLE_GITHUB=1`, `CANOPUS_ALLOW_GITHUB_MUTATION=1`, and the GitHub environment variables are present.
- External mutations (`git push` and `gh pr create`) are dry-run by default. Set `CANOPUS_ENABLE_LIVE_MUTATIONS=1` only in an approved live environment.
- GitHub Project v2 has no official mutation dry-run. Canopus therefore implements an application-level Project mode:
  - `dry-run-offline` (`DryRunOffline`): default; builds local planned GraphQL operations/artifacts only; no HTTP and no mutation.
  - `validate-read-only` (`ValidateReadOnly`): GraphQL queries only; requires `CANOPUS_ENABLE_GITHUB=1` and a token with Project read permission such as `read:project` or equivalent GitHub App permission.
  - `mutate-live` (`MutateLive`): Project v2 add/update mutations; requires `CANOPUS_ENABLE_GITHUB=1`, `CANOPUS_ENABLE_LIVE_MUTATIONS=1`, and `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1`.
- Project sync runs only when Project identity metadata is present (`GITHUB_PROJECT_ID`, `GITHUB_PROJECT_URL`, `GITHUB_PROJECT_ITEM_ID`, or owner kind/owner/number). Setting only a mode or Status field default must not break issue-only flows.
- `GITHUB_PROJECT_ID` means the opaque GraphQL `ProjectV2` node ID. `GITHUB_PROJECT_URL` is optional convenience metadata and may be parsed only from canonical `https://github.com/users/<owner>/projects/<number>` or `https://github.com/orgs/<owner>/projects/<number>` URLs.
- Project Status field/option IDs are environment-specific. Prefer `GITHUB_PROJECT_STATUS_FIELD_ID` and `GITHUB_PROJECT_STATUS_OPTION_ID`; otherwise Canopus can resolve `GITHUB_PROJECT_STATUS_FIELD_NAME` (default `Status`) and `GITHUB_PROJECT_STATUS_OPTION_NAME`/`github_project_status` in read/live modes.
- GitHub Actions `GITHUB_TOKEN` is not sufficient for GitHub Projects access; optional live smoke tests require PAT or GitHub App credentials outside CI.
- `start-pipeline.ps1 -DryRun` validates process wiring without requiring Discord credentials, building binaries, starting long-running services, pushing branches, creating PRs, merging, deploying, or touching live credentials.
- Current live gaps: real Discord delivery, GitHub push/PR/merge/deploy flows, live credential rotation, and production deployment remain outside automated verification.

## Validation

Before changing Canopus behavior, run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 -m py_compile apps/discord-bot/bot.py
python3 -m unittest apps/discord-bot/test_bot_config.py
```

Use targeted tests under `apps/canopus/tests/` for ToolGateway policy, artifact persistence, task backend mapping, workflow transitions, and Project v2 request planning:

```bash
cargo test -p canopus github_project
```
