"""
Discord bot for Dysonsphere pipeline input and approval workflow.
Commands:
  !run <request>          - Add a new task (Pending status)
  !approve [task_id]      - Approve a PendingReview task (Processed)
  !reject  [task_id]      - Reject  a PendingReview task (Failed)
  !status                 - Show all tasks

Authorization: set ALLOWED_USER_IDS=123456789,987654321 in env to restrict access.
If ALLOWED_USER_IDS is empty, all users are allowed (dev mode).
"""
import json
import os
import tempfile
import uuid
import discord
from discord.ext import commands
from dotenv import load_dotenv
from datetime import datetime, timezone

load_dotenv()

DISCORD_BOT_TOKEN = os.environ.get("DISCORD_BOT_TOKEN")
TASKS_JSON_PATH = os.environ.get("TASKS_JSON_PATH", "tasks.json")

_raw_ids = os.environ.get("ALLOWED_USER_IDS", "")
ALLOWED_USER_IDS: set = {
    int(uid.strip()) for uid in _raw_ids.split(",") if uid.strip().isdigit()
}

intents = discord.Intents.default()
intents.message_content = True

bot = commands.Bot(command_prefix="!", intents=intents)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def read_tasks() -> list:
    if not os.path.exists(TASKS_JSON_PATH):
        return []
    try:
        with open(TASKS_JSON_PATH, "r", encoding="utf-8") as f:
            data = json.load(f)
            return data if isinstance(data, list) else []
    except (json.JSONDecodeError, IOError):
        return []


def write_tasks(tasks: list) -> None:
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


def is_authorized(ctx) -> bool:
    if not ALLOWED_USER_IDS:
        return True
    return ctx.author.id in ALLOWED_USER_IDS


@bot.event
async def on_ready():
    print(f"[discord-bot] Logged in as {bot.user} (id={bot.user.id})")
    if ALLOWED_USER_IDS:
        print(f"[discord-bot] Authorized user IDs: {ALLOWED_USER_IDS}")
    else:
        print("[discord-bot] WARNING: ALLOWED_USER_IDS not set — all users can run commands")


@bot.command(name="run")
async def cmd_run(ctx, *, request: str):
    """Add a new task in Pending status."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    tasks = read_tasks()
    task_id = f"discord-{uuid.uuid4().hex[:12]}"
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
async def cmd_approve(ctx, task_id: str = None):
    """Approve a PendingReview task → Processed. Optionally specify task_id."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    tasks = read_tasks()
    pending_review = [t for t in tasks if t.get("meta", {}).get("status") == "PendingReview"]

    if task_id:
        target = next((t for t in tasks if t["task_id"] == task_id), None)
        if not target:
            await ctx.send(f"⚠️ Task `{task_id}` 를 찾을 수 없습니다.")
            return
        if target.get("meta", {}).get("status") != "PendingReview":
            current = target.get("meta", {}).get("status", "?")
            await ctx.send(f"⚠️ Task `{task_id}` 는 PendingReview 상태가 아닙니다 (현재: {current}).")
            return
    else:
        if not pending_review:
            await ctx.send("⚠️ 검토 대기 중인 태스크가 없습니다.")
            return
        if len(pending_review) > 1:
            ids = ", ".join(f"`{t['task_id']}`" for t in pending_review)
            await ctx.send(f"⚠️ 여러 태스크가 검토 대기 중입니다: {ids}\n`!approve <task_id>` 로 지정해주세요.")
            return
        target = pending_review[0]

    for t in tasks:
        if t["task_id"] == target["task_id"]:
            t["meta"]["status"] = "Processed"
            t["meta"]["updated_at"] = now_iso()
            break
    write_tasks(tasks)
    await ctx.send(
        f"✅ 태스크 승인됨\n**ID**: `{target['task_id']}`\n**Status**: Processed"
    )


@bot.command(name="reject")
async def cmd_reject(ctx, task_id: str = None):
    """Reject a PendingReview task → Failed. Optionally specify task_id."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    tasks = read_tasks()
    pending_review = [t for t in tasks if t.get("meta", {}).get("status") == "PendingReview"]

    if task_id:
        target = next((t for t in tasks if t["task_id"] == task_id), None)
        if not target:
            await ctx.send(f"⚠️ Task `{task_id}` 를 찾을 수 없습니다.")
            return
        if target.get("meta", {}).get("status") != "PendingReview":
            current = target.get("meta", {}).get("status", "?")
            await ctx.send(f"⚠️ Task `{task_id}` 는 PendingReview 상태가 아닙니다 (현재: {current}).")
            return
    else:
        if not pending_review:
            await ctx.send("⚠️ 검토 대기 중인 태스크가 없습니다.")
            return
        if len(pending_review) > 1:
            ids = ", ".join(f"`{t['task_id']}`" for t in pending_review)
            await ctx.send(f"⚠️ 여러 태스크가 검토 대기 중입니다: {ids}\n`!reject <task_id>` 로 지정해주세요.")
            return
        target = pending_review[0]

    for t in tasks:
        if t["task_id"] == target["task_id"]:
            t["meta"]["status"] = "Failed"
            t["meta"]["updated_at"] = now_iso()
            break
    write_tasks(tasks)
    await ctx.send(
        f"❌ 태스크 거부됨\n**ID**: `{target['task_id']}`\n**Status**: Failed"
    )


@bot.command(name="status")
async def cmd_status(ctx):
    """Show all tasks with their current status."""
    tasks = read_tasks()
    if not tasks:
        await ctx.send("📋 No tasks in queue.")
        return
    lines = ["📋 **Task Queue**"]
    icon_map = {
        "Pending": "⏳",
        "Dispatched": "🚀",
        "PendingReview": "🔍",
        "Processed": "✅",
        "Failed": "❌",
    }
    for t in tasks:
        status = t.get("meta", {}).get("status", "Unknown")
        task_id = t.get("task_id", "?")
        payload = t.get("payload", "")
        preview = payload[:60] + "…" if len(payload) > 60 else payload
        icon = icon_map.get(status, "❓")
        lines.append(f"{icon} `{task_id}` [{status}] — {preview}")
    await ctx.send("\n".join(lines))


if __name__ == "__main__":
    if not DISCORD_BOT_TOKEN:
        raise RuntimeError("DISCORD_BOT_TOKEN environment variable is not set.")
    bot.run(DISCORD_BOT_TOKEN)
