# Europa — Discord control surface for canopus

Discord-side adapter only. Long-term home: `surfaces/europa/`. Do not add
policy logic here; route mutation to canopus. See
`docs/architecture/boundaries.md`.

## Commands

| Command | Description |
|---------|-------------|
| `!new-project <name> [path]` | Create a project directory, run `git init`, create Discord category/channels, and register it. When `path` is omitted, uses `NEW_PROJECT_DEFAULT_ROOT/<name>` (`/home/sihioov/project/<name>` by default). |
| `!ask <question>` | Ask a direct question without creating a pipeline task |
| `!run <request>` | Add a new Pending task; GitHub-backed projects call Canopus `work-intake` before appending |
| `!approve [task_id]` | Mark a PendingReview task as Processed, then invoke bounded Canopus finalization |
| `!finalize <task_id>` | Retry Canopus finalization for an already-approved Processed task |
| `!reject [task_id]` | Mark a PendingReview task as Failed and block finalization |
| `!propose-approve [task_id]` | For GitHub-backed projects, call Canopus `work-intake` before promoting a PendingProposal to Pending |
| `!propose-reject [task_id]` | Mark a PendingProposal task as Failed |
| `!cancel [task_id]` | Mark a non-terminal task as Failed |
| `!show <task_id>` | Show task details and artifact paths |
| `!status` | Show all tasks and their statuses |
| `!worktree [list|create <name>|switch <name>]` | Show, create via Canopus, or switch project worktrees |

## Pipeline Flow

Tasks flow through these statuses:

1. **PendingProposal** — Candidate discovered by Hubble and awaiting human promotion
2. **Pending** — Initial status when created via `!run` or approved via `!propose-approve`; `!run` payload includes `agenda_id`, role mode, repo path, Discord link, and configured GitHub Issue/Project metadata
3. **PendingReview** — Task awaiting approval/rejection via `!approve` or `!reject` commands
4. **Processed** — Task approved; payload records `approval_state=approved` and `finalize_requested_at` for the Canopus finalization path
5. **Dispatched** — Task sent to Laniakea for execution (TON618 → Laniakea routing)
6. **Failed** — Task rejected or execution failed

## Setup & Run

```bash
pip install -r requirements.txt
cp .env.example .env   # then fill in your DISCORD_BOT_TOKEN
python europa.py
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DISCORD_BOT_TOKEN` | *(required)* | Your Discord bot token |
| `TASKS_JSON_PATH` | *(empty)* | Explicit shared task file path. Set this to the same `tasks.json` watched by TON618/Laniakea for the v1 operator path. |
| `TASKS_DIR` | bot directory | Fallback directory for per-project `tasks-<category_id>.json` files when `TASKS_JSON_PATH` is unset |
| `NEW_PROJECT_DEFAULT_ROOT` | `/home/sihioov/project` | Parent directory used by `!new-project <name>` when the path argument is omitted |
| `ALLOWED_USER_IDS` | *(empty)* | Comma-separated Discord user IDs permitted to use commands; empty = all users allowed in dev mode |
| `CANOPUS_STATE_PATH` | `<repo>/.canopus` | Optional state root used by `!show` when listing artifacts |
| `ASK_COMMAND` | *(empty)* | Optional direct-answer backend for `!ask`; receives the question on stdin and in `STELLARIS_ASK_PROMPT` |
| `ASK_TIMEOUT_SECONDS` | `30` | Timeout for `!ask` backend execution |
| `ASK_MAX_OUTPUT_CHARS` | `1800` | Maximum response characters sent back to Discord |
| `GITHUB_OWNER` / `GITHUB_REPO` | *(empty)* | Repository slug used to populate agenda links and Issue creation URLs in task payloads |
| `GITHUB_PROJECT_ID` | *(empty)* | Optional GitHub Project v2 GraphQL node ID copied into task payloads; this is not a URL |
| `GITHUB_PROJECT_URL` | *(empty)* | Optional canonical `https://github.com/users/<owner>/projects/<number>` or `https://github.com/orgs/<owner>/projects/<number>` URL |
| `GITHUB_PROJECT_OWNER_KIND` / `GITHUB_PROJECT_OWNER` / `GITHUB_PROJECT_NUMBER` | *(empty)* | Optional owner/number lookup metadata used when `GITHUB_PROJECT_ID` is absent |
| `GITHUB_PROJECT_STATUS_FIELD_ID` / `GITHUB_PROJECT_STATUS_FIELD_NAME` | *(empty)* / `Status` | Optional Project v2 Status field identity |
| `GITHUB_PROJECT_STATUS_OPTION_ID` / `GITHUB_PROJECT_STATUS_OPTION_NAME` | *(empty)* | Optional Project v2 Status option identity |
| `CANOPUS_GITHUB_PROJECT_MODE` | `dry-run-offline` | Project v2 mode copied into payloads when configured: `dry-run-offline`, `validate-read-only`, or `mutate-live` |
| `CANOPUS_COMMAND` | `canopus` | Canopus executable used for `project-register`, `work-intake`, and approval finalization subprocess calls |
| `CANOPUS_ALLOW_LOCAL_COMMIT` | `0` | Read by Canopus, not Europa. Set to `1` when approval finalization may create a local commit on the existing task branch. When unset, finalization is dry-run/gate-disabled and no local commit is implied. |
| `CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION` / `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION` | `0` | Required, with `CANOPUS_ENABLE_GITHUB=1` and `CANOPUS_ENABLE_LIVE_MUTATIONS=1`, for GitHub-backed registration/intake |
| `CANOPUS_ALLOW_GITHUB_REPO_CREATE` | `0` | Required in addition to `--create-github-repo`; repo creation is never inferred |
| `CANOPUS_ALLOW_GITHUB_PR_MUTATION` / `CANOPUS_ALLOW_GITHUB_MERGE` / `CANOPUS_ALLOW_DEPLOY` | `0` | Delivery gates for PR creation, merge, and deploy; deploy also requires explicit adapter/environment/command |

Example local echo backend for development:

```bash
ASK_COMMAND="python3 -c 'import sys; print(\"Answer:\", sys.stdin.read())'" python3 europa.py
```

## Shared GitHub-integrated v1 path

For the local v1 pipeline, set `TASKS_JSON_PATH` to the same absolute `tasks.json` consumed by TON618 (`TASKS_JSON_PATH`) and Laniakea (`LANIAKEA_FILE_PATH`). `!run` remains the human request confirmation, creates a unique `agenda_id`, and records GitHub repository/project metadata in the task payload without requiring the Discord bot to push branches or create PRs directly. `!approve` is the final human gate: it moves a `PendingReview` task to `Processed`, records approval timestamps, sets `finalize_requested_at`, and invokes `canopus finalize-approved --tasks <tasks.json> --task-id <task_id> --json`. Canopus owns git policy and sidecars; Europa only reports the structured result. If finalization fails after approval, the approval remains recorded and the operator can retry with `!finalize <task_id>`. `canopus watch` remains optional/background-compatible, but normal Discord approval no longer silently depends on a manually started watcher.

GitHub Project v2 integration is dry-run/offline by default. When `!register`/`!new-project` includes `--github owner/repo --project-owner org:name|user:name`, the bot treats GitHub registration as strict-live: it calls Canopus `project-register` and writes `projects.json` only after Canopus returns complete GitHub IDs/URLs. `!run` and `!propose-approve` call Canopus `work-intake` before appending/promoting tasks; failures leave local task state unchanged or keep proposals in `PendingProposal` with retry metadata. Live Project mutations require all Canopus gates (`CANOPUS_ENABLE_GITHUB=1`, `CANOPUS_ENABLE_LIVE_MUTATIONS=1`, `CANOPUS_ALLOW_GITHUB_REGISTRATION_MUTATION=1`, and `CANOPUS_ALLOW_GITHUB_PROJECT_MUTATION=1`) plus a PAT or GitHub App credential with Projects permissions. GitHub Actions `GITHUB_TOKEN` is not sufficient for Project v2 access. Merge/deploy remain default-off and require the dedicated PR/merge/deploy gates plus explicit deployment config.
