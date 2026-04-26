# Discord Bot — Dysonsphere Pipeline Input

Discord.py 2.x bot for injecting tasks into the Dysonsphere pipeline and approving/rejecting them.

## Commands

| Command | Description |
|---------|-------------|
| `!run <request>` | Add a new task (Pending) to `tasks.json` |
| `!approve [task_id]` | Mark a task as Processed (operates on tasks in PendingReview status) |
| `!reject [task_id]` | Mark a task as Failed (operates on tasks in PendingReview status) |
| `!status` | Show all tasks and their statuses |

## Pipeline Flow

Tasks flow through these statuses:

1. **Pending** — Initial status when created via `!run` command
2. **PendingReview** — Task awaiting approval/rejection via `!approve` or `!reject` commands
3. **Processed** — Task approved; ready for pipeline execution
4. **Dispatched** — Task sent to Laniakea for execution (TON618 → Laniakea routing)
5. **Failed** — Task rejected or execution failed

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
| `TASKS_JSON_PATH` | `tasks.json` | Path to the shared tasks JSON file |
| `ALLOWED_USER_IDS` | *(empty)* | Comma-separated Discord user IDs permitted to use commands; empty = all users allowed in dev mode |
