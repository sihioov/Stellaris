# Discord Bot — Dysonsphere Pipeline Input

Discord.py 2.x bot for injecting tasks into the Dysonsphere pipeline and approving/rejecting them.

## Commands

| Command | Description |
|---------|-------------|
| `!run <request>` | Add a new task (Pending) to `tasks.json` |
| `!approve [task_id]` | Mark a task as Processed (operates on tasks in PendingReview status) |
| `!reject [task_id]` | Mark a task as Failed (operates on tasks in PendingReview status) |
| `!propose-approve [task_id]` | Promote a PendingProposal task to Pending |
| `!propose-reject [task_id]` | Mark a PendingProposal task as Failed |
| `!cancel [task_id]` | Mark a non-terminal task as Failed |
| `!show <task_id>` | Show task details and artifact paths |
| `!status` | Show all tasks and their statuses |

## Pipeline Flow

Tasks flow through these statuses:

1. **PendingProposal** — Candidate discovered by Hubble and awaiting human promotion
2. **Pending** — Initial status when created via `!run` or approved via `!propose-approve`
3. **PendingReview** — Task awaiting approval/rejection via `!approve` or `!reject` commands
4. **Processed** — Task approved; ready for pipeline execution
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
| `TASKS_DIR` | bot directory | Directory for per-project `tasks-<category_id>.json` files |
| `ALLOWED_USER_IDS` | *(empty)* | Comma-separated Discord user IDs permitted to use commands; empty = all users allowed in dev mode |
| `CANOPUS_STATE_PATH` | `<repo>/.canopus` | Optional state root used by `!show` when listing artifacts |
