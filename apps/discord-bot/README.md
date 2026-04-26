# Discord Bot — Dysonsphere Pipeline Input

Discord.py 2.x bot for injecting tasks into the Dysonsphere pipeline and approving/rejecting them.

## Commands

| Command | Description |
|---------|-------------|
| `!run <request>` | Add a new task (Pending) to `tasks.json` |
| `!approve` | Mark the latest task as Processed |
| `!reject` | Mark the latest task as Failed |
| `!status` | Show all tasks and their statuses |

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
