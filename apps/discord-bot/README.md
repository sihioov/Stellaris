# Discord Bot — Dysonsphere Pipeline Input

Discord.py 2.x bot for injecting GitHub-agenda-backed tasks into the Stellaris pipeline and approving/rejecting finalization.

## Commands

| Command | Description |
|---------|-------------|
| `!ask <question>` | Ask a direct question without creating a pipeline task |
| `!run <request>` | Add a new Pending task with agenda/GitHub metadata to the shared task file |
| `!approve [task_id]` | Mark a PendingReview task as Processed and record approval/finalize metadata |
| `!reject [task_id]` | Mark a PendingReview task as Failed and block finalization |
| `!propose-approve [task_id]` | Promote a PendingProposal task to Pending |
| `!propose-reject [task_id]` | Mark a PendingProposal task as Failed |
| `!cancel [task_id]` | Mark a non-terminal task as Failed |
| `!show <task_id>` | Show task details and artifact paths |
| `!status` | Show all tasks and their statuses |

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
python bot.py
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DISCORD_BOT_TOKEN` | *(required)* | Your Discord bot token |
| `TASKS_JSON_PATH` | *(empty)* | Explicit shared task file path. Set this to the same `tasks.json` watched by TON618/Laniakea for the v1 operator path. |
| `TASKS_DIR` | bot directory | Fallback directory for per-project `tasks-<category_id>.json` files when `TASKS_JSON_PATH` is unset |
| `ALLOWED_USER_IDS` | *(empty)* | Comma-separated Discord user IDs permitted to use commands; empty = all users allowed in dev mode |
| `CANOPUS_STATE_PATH` | `<repo>/.canopus` | Optional state root used by `!show` when listing artifacts |
| `ASK_COMMAND` | *(empty)* | Optional direct-answer backend for `!ask`; receives the question on stdin and in `STELLARIS_ASK_PROMPT` |
| `ASK_TIMEOUT_SECONDS` | `30` | Timeout for `!ask` backend execution |
| `ASK_MAX_OUTPUT_CHARS` | `1800` | Maximum response characters sent back to Discord |
| `GITHUB_OWNER` / `GITHUB_REPO` | *(empty)* | Repository slug used to populate agenda links and Issue creation URLs in task payloads |
| `GITHUB_PROJECT_ID` / `GITHUB_PROJECT_URL` | *(empty)* | Optional GitHub Project metadata copied into task payloads/status output |

Example local echo backend for development:

```bash
ASK_COMMAND="python3 -c 'import sys; print(\"Answer:\", sys.stdin.read())'" python3 bot.py
```

## Shared GitHub-integrated v1 path

For the local v1 pipeline, set `TASKS_JSON_PATH` to the same absolute `tasks.json` consumed by TON618 (`TASKS_JSON_PATH`) and Laniakea (`LANIAKEA_FILE_PATH`). `!run` remains the human request confirmation, creates a unique `agenda_id`, and records GitHub repository/project metadata in the task payload without requiring the Discord bot to push branches or create PRs directly. `!approve` is the final human gate: it moves a `PendingReview` task to `Processed`, records approval timestamps, and sets `finalize_requested_at` for the Canopus finalization owner. `!reject` records rejection metadata and prevents finalization.
