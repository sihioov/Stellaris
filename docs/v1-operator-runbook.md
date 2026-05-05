# V1 Operator Runbook

Purpose: close the V1 self-hosting loop on mock/offline runtime, with one optional GitHub Project **validate-read-only** probe. Live mutation, PR creation, merge, and deploy stay outside this closure stack.

## 1. Environment keys

Copy `.env.example` to `.env` and keep defaults non-mutating unless a later live ramp-up spec says otherwise.

| Key | Required for dry-run | Required for validate-read-only | Meaning |
|---|:--:|:--:|---|
| `GITHUB_TOKEN` | No | Yes | Token used only for GitHub read calls in validate-read-only mode. |
| `GITHUB_OWNER` | No | Yes | Repository owner for GitHub issue/project lookup. |
| `GITHUB_REPO` | No | Yes | Repository name for GitHub issue/project lookup. |
| `GITHUB_PROJECT_ID` | No | Yes | GitHub ProjectV2 GraphQL node ID. |
| `GITHUB_PROJECT_URL` | No | Optional | Human URL for the project; metadata/display fallback. |
| `GITHUB_PROJECT_OWNER_KIND` | No | Optional | `org` or `user` owner kind for project lookup when ID is not enough. |
| `GITHUB_PROJECT_OWNER` | No | Optional | Project owner login. |
| `GITHUB_PROJECT_NUMBER` | No | Optional | Project number for owner/number lookup. |
| `GITHUB_PROJECT_STATUS_FIELD_ID` | No | Optional | Status field node ID; avoids field lookup. |
| `GITHUB_PROJECT_STATUS_FIELD_NAME` | No | Optional | Status field name, usually `Status`. |
| `GITHUB_PROJECT_STATUS_OPTION_ID` | No | Optional | Status option node ID; avoids option lookup. |
| `GITHUB_PROJECT_STATUS_OPTION_NAME` | No | Optional | Desired status option name. |
| `CANOPUS_GITHUB_PROJECT_MODE` | No | Yes | Use `dry-run-offline` by default; C4 probe sets `validate-read-only`. |
| `CANOPUS_ENABLE_GITHUB` | No | Yes | Must be `1` for validate-read-only HTTP reads. |
| `CANOPUS_ALLOW_GITHUB_MUTATION` | No | No | Legacy mutation request flag; keep `0` for V1 closure. |
| `CANOPUS_ENABLE_LIVE_MUTATIONS` | No | No | Global live mutation gate; keep `0` for V1 closure. |
| `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION` | No | No | Project mutation gate; keep `0` for validate-read-only. |
| `CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION` | No | No | Project/repo registration mutation gate; keep `0`. |
| `CANOPUS_ALLOW_GITHUB_REPO_CREATE` | No | No | Repository creation gate; keep `0`. |
| `CANOPUS_ALLOW_GITHUB_PR_MUTATION` | No | No | PR creation/update gate; keep `0`. |
| `CANOPUS_ALLOW_GITHUB_MERGE` | No | No | Merge gate; keep `0`. |
| `CANOPUS_ALLOW_DEPLOY` | No | No | Deploy gate; keep `0`. |
| `CANOPUS_DEPLOY_ADAPTER` | No | No | Deploy adapter name for later live ramp-up only. |
| `CANOPUS_DEPLOY_ENVIRONMENT` | No | No | Deploy target for later live ramp-up only. |
| `CANOPUS_DEPLOY_COMMAND` | No | No | Deploy command for later live ramp-up only. |
| `CANOPUS_COMMAND` | Optional | Optional | Canopus executable; defaults to `canopus`. |
| `DISCORD_WEBHOOK_URL` | No | No | Optional notification webhook; omit for offline tests. |
| `TASKS_JSON_PATH` | Yes | No | Shared queue file for Europa, TON618, and Laniakea. |
| `LANIAKEA_SOURCE` | Yes | No | Use `file` for V1 closure. |
| `LANIAKEA_FILE_PATH` | Yes | No | Same file as `TASKS_JSON_PATH`. |
| `CANOPUS_AGENT_RUNTIME` | Optional | No | Set `command` to test command runtime; unset uses mock runtime. |
| `CANOPUS_AGENT_COMMAND` | Optional | No | Deterministic local command used by command runtime. |
| `CANOPUS_REPO` | Optional | No | Repo path for `canopus watch`/`finalize`. |
| `CANOPUS_STATE` | Optional | No | Canopus state directory, usually `.canopus`. |
| `REPO_PATH` | Optional | No | Repo scanned by Kepler/Hubble-style discovery. |
| `RUST_LOG` | Optional | No | Rust logging level. |

The validate-read-only helper also accepts `GITHUB_PROJECT_ITEM_ID` or `GITHUB_ISSUE_NUMBER` through the process environment or parameters. These are probe inputs, not live mutation gates.

## 2. Dry-run launcher branch

Inspect the process wiring without credentials:

```powershell
pwsh ./start-pipeline.ps1 -DryRun
```

Expected output lists TON618, Laniakea, Canopus watch/finalizer, Kepler, Discord Bot, and the validate-read-only helper. This branch must not require tokens and must not push, create PRs, mutate GitHub Issues, or mutate GitHub Projects.

## 3. Live launcher branch for V1 closure

Run the local pipeline with live mutation gates still closed:

```powershell
pwsh ./start-pipeline.ps1
```

Before starting long-lived processes, the launcher invokes:

```powershell
pwsh ./scripts/validate-read-only.ps1 -Repo . -State ./.canopus
```

If token/project/probe inputs are missing, the helper warns and exits successfully. If inputs exist, it calls `canopus submit` with `CANOPUS_GITHUB_PROJECT_MODE=validate-read-only`, `CANOPUS_ENABLE_GITHUB=1`, and all mutation/PR/merge/deploy gates set to `0`.

Important: validate-read-only is **GitHub-read-only, not workflow-read-only**. `canopus submit` may create local branch/artifact/run-record side effects in a disposable or clean repo, but it must not construct or execute GitHub Project mutations.

## 4. Approve/reject criteria

Review these artifacts before approving a task:

- `.canopus/runs/<run_id>.json` — stage records; require successful `plan`, `code`/role stage, `check`, and `complete` records.
- `.canopus/artifacts/<task-or-stage>/plan.md` — planned change summary.
- `.canopus/artifacts/<task-or-stage>/runtime-log.md` — runtime or command output.
- `.canopus/artifacts/<task-or-stage>/test-result.md` — validation output when produced.
- `.canopus/artifacts/<task-or-stage>/review.md` — reviewer output.

Approve only when stage records and artifacts match the requested scope and no live mutation gate was required. Reject when the task is off-scope, validation is missing/failed, artifacts are inconsistent, or a gate violation appears.

## 5. Finalize and delivery-gate dry-run

After approval moves a task to `Processed`, run one finalizer tick:

```bash
CANOPUS_REPO=. CANOPUS_STATE=.canopus canopus watch --once tasks.json
```

Expected closure artifacts:

- `.canopus/runs/<run_id>-finalize.txt`
- `.canopus/runs/<run_id>-delivery-gate.json` after PR-C5

The finalize record is dry-run evidence. It must state that git add/commit/push, `gh pr create`, and issue close were skipped unless a later live ramp-up spec explicitly opens those gates.

## 6. Failure recovery

- **Stuck `Pending` task**: confirm TON618 is reading `TASKS_JSON_PATH`; run a bounded dispatch/smoke test or restart TON618.
- **Stuck `Dispatched` task**: confirm Laniakea has `LANIAKEA_SOURCE=file`, `LANIAKEA_FILE_PATH=$TASKS_JSON_PATH`, `CANOPUS_REPO_PATH`, and `CANOPUS_STATE_PATH`.
- **Stuck `PendingReview` task**: inspect run records/artifacts, then use the Discord approval/rejection command or update through the approved Canopus/Europa path.
- **Missing finalize record**: verify task status is `Processed`, then rerun `canopus watch --once` with the correct state and task file.
- **validate-read-only skipped**: set the missing token/project/probe env keys or pass `-GitHubProjectItemId` / `-GitHubIssueNumber`, then rerun `scripts/validate-read-only.ps1`.
- **validate-read-only fails**: keep mutation gates closed, inspect the error, and rerun with read permissions only. Do not switch to `mutate-live` inside this V1 closure stack.

## 7. Manual validate-read-only probe

Credentialed one-shot check:

```powershell
$env:CANOPUS_ENABLE_GITHUB = "1"
$env:CANOPUS_GITHUB_PROJECT_MODE = "validate-read-only"
$env:CANOPUS_ENABLE_LIVE_MUTATIONS = "0"
$env:CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION = "0"
pwsh ./scripts/validate-read-only.ps1 -Repo . -State ./.canopus -GitHubProjectItemId "<PVTI_node_id>"
```

Alternative when probing by issue number:

```powershell
pwsh ./scripts/validate-read-only.ps1 -Repo . -State ./.canopus -GitHubIssueNumber 123
```

## 8. Live mutation transition procedure — out of scope

Do not enable these during V1 closure:

- `CANOPUS_ENABLE_LIVE_MUTATIONS=1`
- `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1`
- `CANOPUS_ALLOW_GITHUB_PR_MUTATION=1`
- `CANOPUS_ALLOW_GITHUB_MERGE=1`
- `CANOPUS_ALLOW_DEPLOY=1`

The separate final ramp-up spec must define disposable resources, credentials, expected PR/project mutations, rollback, and audit evidence before any live mutation run.

## 9. V2 entry procedure

After V1 closure and the separate final ramp-up are complete, switch from mock runtime to command runtime for V2 handoff:

```bash
export CANOPUS_AGENT_RUNTIME=command
export CANOPUS_AGENT_COMMAND='<agent command that reads CANOPUS_* env and writes deterministic output>'
```

Keep `CANOPUS_AGENT_RUNTIME` unset for V1 mock evidence. The `AgentRunResult.message_log` schema is reserved in V1; actual message persistence is V2 work.

## 10. 2026-05-05 live Discord → GitHub intake slice

This is the minimal live slice intended for the first Discord-operated deployment. It permits **GitHub Issue creation for Discord work intake** while keeping GitHub Project sync optional and data-gated.

### Supported operator path

1. Register a Discord category/project with GitHub owner/repo metadata.
2. Use `!run <request>` or `!propose-approve <task_id>` from Discord.
3. Europa calls `canopus work-intake` when the project registration has `github_owner` and `github_repo`.
4. Canopus creates a GitHub Issue and returns issue metadata to the task payload.
5. If project sync data and gates are complete, Canopus may also sync Project v2 according to `--project-sync`; otherwise issue creation still succeeds in best-effort mode.
6. Use `!approve <task_id>` only after review. Approval writes `approval_state=approved`, `finalize_requested_at`, and Discord provenance (`approved_by`, `approval_source=discord`, `approval_message_url`).
7. `canopus watch --once <tasks.json>` finalizes only `Processed` tasks that contain both approval evidence and a finalize request in the decoded payload.

### Required gates for Issue creation

```bash
export CANOPUS_ENABLE_GITHUB=1
export CANOPUS_ENABLE_LIVE_MUTATIONS=1
export CANOPUS_ALLOW_GITHUB_MUTATION=1
export GITHUB_TOKEN='<token with repo issue permission>'
```

`work-intake` does **not** use fake owner/repo defaults. The project registration must include `github_owner` and `github_repo`; otherwise the command fails before creating an Issue.

### Project sync policy

`canopus work-intake` accepts:

- `--project-sync off` — never sync Project v2.
- `--project-sync best-effort` — default. Sync only when all project identity, status target, created Issue number, and `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1` are present. Missing data skips Project sync but keeps the Issue.
- `--project-sync required` — preflight before Issue creation. Fails unless project identity, status target, `CANOPUS_ENABLE_GITHUB=1`, `CANOPUS_ENABLE_LIVE_MUTATIONS=1`, and `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1` are present.

If Project sync is attempted after Issue creation and then fails, Canopus exits nonzero but writes one stdout JSON object with `ok=false` plus the created Issue metadata. Europa preserves that object so operators can see the Issue that already exists.
