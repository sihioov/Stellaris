"""
Discord bot for Dysonsphere pipeline input and approval workflow.
Commands:
  !run <request>  - Add a new task (Pending status)
  !approve        - Mark the latest task as Processed
  !reject         - Mark the latest task as Failed
  !status         - Show all tasks
"""
import json
import os
import tempfile
import time
import discord
from discord.ext import commands
from dotenv import load_dotenv
from datetime import datetime, timezone

load_dotenv()

DISCORD_BOT_TOKEN = os.environ.get("DISCORD_BOT_TOKEN")
TASKS_JSON_PATH = os.environ.get("TASKS_JSON_PATH", "tasks.json")

intents = discord.Intents.default()
intents.message_content = True

bot = commands.Bot(command_prefix="!", intents=intents)


def now_iso() -> str:
    """Return current UTC time in ISO 8601 format."""
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def read_tasks() -> list:
    """Read tasks from JSON file, returning empty list if file doesn't exist."""
    if not os.path.exists(TASKS_JSON_PATH):
        return []
    try:
        with open(TASKS_JSON_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
            return data if isinstance(data, list) else []
    except (json.JSONDecodeError, IOError):
        return []


def write_tasks(tasks: list) -> None:
    """Atomically write tasks to JSON file using temp file + rename."""
    dir_path = os.path.dirname(os.path.abspath(TASKS_JSON_PATH))
    os.makedirs(dir_path, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w",
        encoding="utf-8",
        dir=dir_path,
        delete=False,
        suffix=".tmp",
    ) as tmp:
        json.dump(tasks, tmp, ensure_ascii=False, indent=2)
        tmp_path = tmp.name
    os.replace(tmp_path, TASKS_JSON_PATH)


@bot.event
async def on_ready():
    print(f"[discord-bot] Logged in as {bot.user} (id={bot.user.id})")


@bot.command(name="run")
async def cmd_run(ctx, *, request: str):
    """Add a new task in Pending status."""
    tasks = read_tasks()
    task_id = f"discord-{int(time.time() * 1000)}"
    ts = now_iso()
    task = {
        "task_id": task_id,
        "task_type": {"Custom": "canopus.agent"},
        "payload": request,
        "meta": {
            "status": "Pending",
            "created_at": ts,
            "updated_at": ts,
        },
    }
    tasks.append(task)
    write_tasks(tasks)
    await ctx.send(
        f"✅ Task added\n"
        f"**ID**: `{task_id}`\n"
        f"**Payload**: {request}\n"
        f"**Status**: Pending"
    )


@bot.command(name="approve")
async def cmd_approve(ctx):
    """Mark the latest task as Processed."""
    tasks = read_tasks()
    if not tasks:
        await ctx.send("⚠️ No tasks found.")
        return
    latest = tasks[-1]
    latest["meta"]["status"] = "Processed"
    latest["meta"]["updated_at"] = now_iso()
    write_tasks(tasks)
    await ctx.send(
        f"✅ Task approved\n"
        f"**ID**: `{latest['task_id']}`\n"
        f"**Status**: Processed"
    )


@bot.command(name="reject")
async def cmd_reject(ctx):
    """Mark the latest task as Failed."""
    tasks = read_tasks()
    if not tasks:
        await ctx.send("⚠️ No tasks found.")
        return
    latest = tasks[-1]
    latest["meta"]["status"] = "Failed"
    latest["meta"]["updated_at"] = now_iso()
    write_tasks(tasks)
    await ctx.send(
        f"❌ Task rejected\n"
        f"**ID**: `{latest['task_id']}`\n"
        f"**Status**: Failed"
    )


@bot.command(name="status")
async def cmd_status(ctx):
    """Show all tasks with their current status."""
    tasks = read_tasks()
    if not tasks:
        await ctx.send("📋 No tasks in queue.")
        return
    lines = ["📋 **Task Queue**"]
    for t in tasks:
        status = t.get("meta", {}).get("status", "Unknown")
        task_id = t.get("task_id", "?")
        payload = t.get("payload", "")
        preview = payload[:60] + "…" if len(payload) > 60 else payload
        icon = {"Pending": "⏳", "Processed": "✅", "Failed": "❌"}.get(status, "❓")
        lines.append(f"{icon} `{task_id}` [{status}] — {preview}")
    await ctx.send("\n".join(lines))


if __name__ == "__main__":
    if not DISCORD_BOT_TOKEN:
        raise RuntimeError("DISCORD_BOT_TOKEN environment variable is not set.")
    bot.run(DISCORD_BOT_TOKEN)
