"""
Discord bot for Stellaris AI pipeline.

Project structure:
  Discord Category = Git project (repo)
  Channels per category:
    #general     - status, !register, !new-project
    #planning    - Planner agent only
    #development - Full pipeline (Planner+Coder+Reviewer)
    #review      - Reviewer agent only

Commands:
  !new-project <name> <path>  - Create category + 4 channels + register
  !register <path>            - Register current category's repo path
  !run <request>              - Add task (agent type from channel name)
  !approve [task_id]          - Approve PendingReview task
  !reject  [task_id]          - Reject  PendingReview task
  !propose-approve [task_id]  - Promote PendingProposal task to Pending
  !propose-reject  [task_id]  - Reject PendingProposal task as Failed
  !cancel [task_id]           - Cancel a non-terminal task as Failed
  !show [task_id]             - Show task details and artifact paths
  !status                     - Show tasks for current project

Authorization: set ALLOWED_USER_IDS=123456789,987654321 in env to restrict access.
"""
import json
import os
import tempfile
import uuid
from contextlib import contextmanager

try:
    import fcntl
except ImportError:  # pragma: no cover - Windows/dev fallback
    fcntl = None

import discord
from discord.ext import commands
from dotenv import load_dotenv
from datetime import datetime, timezone

load_dotenv()

DISCORD_BOT_TOKEN = os.environ.get("DISCORD_BOT_TOKEN")
PROJECTS_JSON_PATH = os.environ.get(
    "PROJECTS_JSON_PATH",
    os.path.join(os.path.dirname(__file__), "projects.json"),
)
TASKS_DIR = os.environ.get("TASKS_DIR", os.path.dirname(__file__))
CANOPUS_STATE_PATH = os.environ.get("CANOPUS_STATE_PATH")

_raw_ids = os.environ.get("ALLOWED_USER_IDS", "")
ALLOWED_USER_IDS: set = {
    int(uid.strip()) for uid in _raw_ids.split(",") if uid.strip().isdigit()
}

CHANNEL_TYPE_MAP = {
    "planning": "canopus.planner",
    "development": "canopus.agent",
    "review": "canopus.reviewer",
    "general": None,
}

intents = discord.Intents.default()
intents.message_content = True
intents.guilds = True

bot = commands.Bot(command_prefix="!", intents=intents, help_command=None)

ICON_MAP = {
    "Pending": "⏳",
    "Dispatched": "🚀",
    "PendingReview": "🔍",
    "Processed": "✅",
    "Failed": "❌",
    "PendingProposal": "📝",
}


# ── helpers ──────────────────────────────────────────────────────────────────

def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def read_projects() -> dict:
    if not os.path.exists(PROJECTS_JSON_PATH):
        return {"projects": {}}
    try:
        with open(PROJECTS_JSON_PATH, "r", encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, IOError):
        return {"projects": {}}


def write_projects(data: dict) -> None:
    dir_path = os.path.dirname(os.path.abspath(PROJECTS_JSON_PATH))
    os.makedirs(dir_path, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=dir_path, delete=False, suffix=".tmp"
    ) as tmp:
        json.dump(data, tmp, ensure_ascii=False, indent=2)
        tmp_path = tmp.name
    os.replace(tmp_path, PROJECTS_JSON_PATH)


def get_project(category_id: int) -> dict | None:
    data = read_projects()
    return data["projects"].get(str(category_id))


def get_tasks_path(category_id: int) -> str:
    return os.path.join(TASKS_DIR, f"tasks-{category_id}.json")


def get_channel_type(channel_name: str) -> str | None:
    name = channel_name.lower().strip()
    return CHANNEL_TYPE_MAP.get(name)



def lock_path_for(path: str) -> str:
    root, ext = os.path.splitext(path)
    return f"{root}.lock" if ext else f"{path}.lock"


@contextmanager
def task_file_lock(path: str, exclusive: bool):
    lock_path = lock_path_for(path)
    os.makedirs(os.path.dirname(os.path.abspath(lock_path)), exist_ok=True)
    with open(lock_path, "a+", encoding="utf-8") as lock_file:
        if fcntl is not None:
            lock_mode = fcntl.LOCK_EX if exclusive else fcntl.LOCK_SH
            fcntl.flock(lock_file.fileno(), lock_mode)
        try:
            yield
        finally:
            if fcntl is not None:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)

def _read_tasks_unlocked(path: str) -> list:
    if not os.path.exists(path):
        return []
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
            return data if isinstance(data, list) else []
    except (json.JSONDecodeError, IOError):
        return []


def _write_tasks_unlocked(tasks: list, path: str) -> None:
    dir_path = os.path.dirname(os.path.abspath(path))
    os.makedirs(dir_path, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=dir_path, delete=False, suffix=".tmp"
    ) as tmp:
        json.dump(tasks, tmp, ensure_ascii=False, indent=2)
        tmp_path = tmp.name
    os.replace(tmp_path, path)


def read_tasks(path: str) -> list:
    with task_file_lock(path, exclusive=False):
        return _read_tasks_unlocked(path)


def write_tasks(tasks: list, path: str) -> None:
    with task_file_lock(path, exclusive=True):
        _write_tasks_unlocked(tasks, path)



def append_task_locked(path: str, task: dict) -> None:
    with task_file_lock(path, exclusive=True):
        tasks = _read_tasks_unlocked(path)
        tasks.append(task)
        _write_tasks_unlocked(tasks, path)


def update_task_status_locked(
    path: str,
    task_id: str | None,
    command: str,
    statuses: set[str],
    label: str,
    next_status: str,
) -> tuple[dict | None, str | None]:
    with task_file_lock(path, exclusive=True):
        tasks = _read_tasks_unlocked(path)
        candidates = [t for t in tasks if t.get("meta", {}).get("status") in statuses]
        if task_id:
            target = next((t for t in tasks if t.get("task_id") == task_id), None)
            if not target:
                return None, f"⚠️ Task `{task_id}` 를 찾을 수 없습니다."
            current = target.get("meta", {}).get("status", "?")
            if current not in statuses:
                return None, f"⚠️ Task `{task_id}` 는 {label} 상태가 아닙니다 (현재: {current})."
        else:
            if not candidates:
                return None, f"⚠️ {label} 상태의 태스크가 없습니다."
            if len(candidates) > 1:
                ids = ", ".join(f"`{t['task_id']}`" for t in candidates)
                return None, f"⚠️ 여러 태스크가 대상입니다: {ids}\n`!{command} <task_id>` 로 지정해주세요."
            target = candidates[0]

        target.setdefault("meta", {})["status"] = next_status
        target["meta"]["updated_at"] = now_iso()
        snapshot = json.loads(json.dumps(target))
        _write_tasks_unlocked(tasks, path)
        return snapshot, None


def is_authorized(ctx) -> bool:
    if not ALLOWED_USER_IDS:
        return True
    return ctx.author.id in ALLOWED_USER_IDS



def _payload_data(task: dict) -> dict:
    payload = task.get("payload", "")
    if isinstance(payload, dict):
        return payload
    try:
        parsed = json.loads(payload)
        return parsed if isinstance(parsed, dict) else {"raw": payload}
    except (json.JSONDecodeError, TypeError):
        return {"raw": payload}


def _artifact_paths(project: dict | None, task: dict) -> list[str]:
    state_root = CANOPUS_STATE_PATH
    if not state_root and project:
        state_root = os.path.join(project.get("repo_path", ""), ".canopus")
    if not state_root:
        return []

    task_id = task.get("task_id", "")
    candidates = [
        os.path.join(state_root, "artifacts", task_id),
        os.path.join(state_root, "runs", f"{task_id}.json"),
    ]
    found = []
    for path in candidates:
        if os.path.isdir(path):
            for root, _, files in os.walk(path):
                for name in files:
                    found.append(os.path.join(root, name))
        elif os.path.exists(path):
            found.append(path)
    return found[:10]

def get_category_context(ctx):
    """Returns (category_id, project, tasks_path) or (None, None, None) if not in a project channel."""
    if not ctx.guild or not hasattr(ctx.channel, "category") or not ctx.channel.category:
        return None, None, None
    category_id = ctx.channel.category.id
    project = get_project(category_id)
    tasks_path = get_tasks_path(category_id)
    return category_id, project, tasks_path


# ── events ───────────────────────────────────────────────────────────────────

@bot.event
async def on_ready():
    print(f"[discord-bot] Logged in as {bot.user} (id={bot.user.id})")
    projects = read_projects()
    count = len(projects["projects"])
    print(f"[discord-bot] {count} project(s) registered in projects.json")
    if ALLOWED_USER_IDS:
        print(f"[discord-bot] Authorized user IDs: {ALLOWED_USER_IDS}")
    else:
        print("[discord-bot] WARNING: ALLOWED_USER_IDS not set — all users can run commands")


# ── commands ──────────────────────────────────────────────────────────────────

@bot.command(name="new-project")
async def cmd_new_project(ctx, name: str = None, *, repo_path: str = None):
    """Create directory, git init, Discord category + 4 channels, register in projects.json."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not name or not repo_path:
        await ctx.send("사용법: `!new-project <이름> <로컬경로>`\n예: `!new-project my-app C:/projects/my-app`")
        return
    if not ctx.guild:
        await ctx.send("⚠️ 서버 채널에서만 사용할 수 있습니다.")
        return

    # 중복 체크: 같은 경로가 이미 등록됐는지
    data = read_projects()
    for cat_id, proj in data["projects"].items():
        if os.path.normpath(proj["repo_path"]) == os.path.normpath(repo_path):
            await ctx.send(
                f"⚠️ 해당 경로는 이미 **{proj['name']}** 프로젝트로 등록되어 있습니다.\n"
                f"경로: `{proj['repo_path']}`\n"
                f"기존 프로젝트를 사용하거나 `!register <경로>` 로 다른 카테고리에 연결하세요."
            )
            return

    # 중복 체크: 같은 이름의 Discord 카테고리가 이미 있는지
    existing_category = discord.utils.get(ctx.guild.categories, name=name)
    if existing_category:
        await ctx.send(
            f"⚠️ **{name}** 카테고리가 이미 Discord 서버에 존재합니다.\n"
            f"기존 카테고리를 사용하려면 해당 채널에서 `!register {repo_path}` 를 실행하세요."
        )
        return

    steps = []
    category = None
    try:
        # 1. 디렉토리 생성
        if os.path.exists(repo_path):
            steps.append(f"📂 디렉토리 이미 존재: `{repo_path}`")
        else:
            os.makedirs(repo_path, exist_ok=True)
            steps.append(f"📂 디렉토리 생성: `{repo_path}`")

        # 2. git init (이미 git 레포면 skip)
        git_dir = os.path.join(repo_path, ".git")
        if os.path.isdir(git_dir):
            steps.append("🔧 Git 레포지토리 이미 존재 (init skip)")
        else:
            import subprocess
            result = subprocess.run(
                ["git", "init", repo_path],
                capture_output=True, text=True
            )
            if result.returncode != 0:
                await ctx.send(f"❌ git init 실패: {result.stderr}")
                return
            steps.append("🔧 `git init` 완료")

        # 3. Discord 카테고리 + 채널 생성
        category = await ctx.guild.create_category(name)
        for ch_name in ("general", "planning", "development", "review"):
            await category.create_text_channel(ch_name)
        steps.append(f"💬 Discord 카테고리 + 4채널 생성")

        # 4. projects.json 등록
        data = read_projects()
        data["projects"][str(category.id)] = {
            "name": name,
            "repo_path": repo_path,
            "registered_at": now_iso(),
        }
        write_projects(data)
        steps.append("📝 projects.json 등록 완료")

        step_list = "\n".join(steps)
        await ctx.send(
            f"✅ **프로젝트 생성 완료**: {name}\n\n"
            f"{step_list}\n\n"
            f"이제 #development 채널에서 `!run <요청>` 으로 작업을 시작하세요."
        )
    except discord.Forbidden:
        await ctx.send("❌ 채널 생성 권한(Manage Channels)이 없습니다. 봇 권한을 확인해주세요.")
    except Exception as e:
        await ctx.send(f"❌ 프로젝트 생성 실패: {e}")


@bot.command(name="register")
async def cmd_register(ctx, *, repo_path: str = None):
    """Register current category's repo path in projects.json."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not repo_path:
        await ctx.send("사용법: `!register <로컬경로>`\n예: `!register D:/develop/repositories/Stellaris`")
        return
    if not os.path.isdir(repo_path):
        await ctx.send(f"❌ 경로가 존재하지 않습니다: `{repo_path}`\n신규 프로젝트라면 `!new-project <이름> <경로>` 를 사용하세요.")
        return
    if not os.path.isdir(os.path.join(repo_path, ".git")):
        await ctx.send(f"❌ Git 레포지토리가 아닙니다: `{repo_path}`\n`git init`을 먼저 실행하거나 `!new-project` 를 사용하세요.")
        return
    if not ctx.guild or not hasattr(ctx.channel, "category") or not ctx.channel.category:
        await ctx.send("⚠️ 카테고리가 있는 채널에서만 사용할 수 있습니다.")
        return

    category = ctx.channel.category
    data = read_projects()
    is_update = str(category.id) in data["projects"]
    data["projects"][str(category.id)] = {
        "name": category.name,
        "repo_path": repo_path,
        "registered_at": now_iso(),
    }
    write_projects(data)

    action = "업데이트됨" if is_update else "등록됨"
    await ctx.send(
        f"✅ **프로젝트 {action}**\n"
        f"📁 카테고리: **{category.name}**\n"
        f"📂 Repo: `{repo_path}`"
    )


@bot.command(name="run")
async def cmd_run(ctx, *, request: str = None):
    """Add a new task. Agent type is determined by channel name."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not request:
        await ctx.send("사용법: `!run <요청내용>`")
        return

    category_id, project, tasks_path = get_category_context(ctx)

    if category_id is None:
        await ctx.send("⚠️ 카테고리가 있는 채널에서만 사용할 수 있습니다.")
        return
    if project is None:
        await ctx.send(
            f"⚠️ 이 채널의 카테고리에 등록된 프로젝트가 없습니다.\n"
            f"`!register <로컬경로>` 또는 `!new-project <이름> <경로>` 를 먼저 실행해주세요."
        )
        return

    channel_type = get_channel_type(ctx.channel.name)
    if channel_type is None:
        await ctx.send("⚠️ `#planning`, `#development`, `#review` 채널에서만 작업을 실행할 수 있습니다.")
        return

    task_id = f"discord-{uuid.uuid4().hex[:12]}"
    ts = now_iso()
    task = {
        "task_id": task_id,
        "task_type": {"Custom": channel_type},
        "payload": json.dumps({"request": request, "repo_path": project["repo_path"]}, ensure_ascii=False),
        "meta": {
            "status": "Pending",
            "created_at": ts,
            "updated_at": ts,
        },
    }
    append_task_locked(tasks_path, task)

    type_label = {"canopus.planner": "📋 Planning", "canopus.agent": "🔄 Full Pipeline", "canopus.reviewer": "🔍 Review"}.get(channel_type, channel_type)
    await ctx.send(
        f"✅ **Task 추가됨**\n"
        f"**ID**: `{task_id}`\n"
        f"**프로젝트**: {project['name']}\n"
        f"**타입**: {type_label}\n"
        f"**요청**: {request}\n"
        f"**Status**: Pending"
    )


@bot.command(name="approve")
async def cmd_approve(ctx, task_id: str = None):
    """Approve a PendingReview task → Processed."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    target, error = update_task_status_locked(
        tasks_path, task_id, "approve", {"PendingReview"}, "PendingReview", "Processed"
    )
    if error:
        await ctx.send(error)
        return

    await ctx.send(f"✅ 태스크 승인됨\n**ID**: `{target['task_id']}`\n**Status**: Processed")


@bot.command(name="reject")
async def cmd_reject(ctx, task_id: str = None):
    """Reject a PendingReview task → Failed."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    target, error = update_task_status_locked(
        tasks_path, task_id, "reject", {"PendingReview"}, "PendingReview", "Failed"
    )
    if error:
        await ctx.send(error)
        return

    await ctx.send(f"❌ 태스크 거부됨\n**ID**: `{target['task_id']}`\n**Status**: Failed")


@bot.command(name="propose-approve")
async def cmd_propose_approve(ctx, task_id: str = None):
    """Approve a PendingProposal task → Pending."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    target, error = update_task_status_locked(
        tasks_path,
        task_id,
        "propose-approve",
        {"PendingProposal"},
        "PendingProposal",
        "Pending",
    )
    if error:
        await ctx.send(error)
        return

    await ctx.send(f"✅ 후보 승인됨\n**ID**: `{target['task_id']}`\n**Status**: Pending")


@bot.command(name="propose-reject")
async def cmd_propose_reject(ctx, task_id: str = None):
    """Reject a PendingProposal task → Failed."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    target, error = update_task_status_locked(
        tasks_path,
        task_id,
        "propose-reject",
        {"PendingProposal"},
        "PendingProposal",
        "Failed",
    )
    if error:
        await ctx.send(error)
        return

    await ctx.send(f"❌ 후보 거부됨\n**ID**: `{target['task_id']}`\n**Status**: Failed")


@bot.command(name="cancel")
async def cmd_cancel(ctx, task_id: str = None):
    """Cancel any non-terminal task → Failed. Processed tasks cannot be cancelled."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    cancelable = {"Pending", "Dispatched", "PendingReview", "PendingProposal"}
    target, error = update_task_status_locked(
        tasks_path, task_id, "cancel", cancelable, "취소 가능", "Failed"
    )
    if error:
        await ctx.send(error)
        return

    await ctx.send(f"🛑 태스크 취소됨\n**ID**: `{target['task_id']}`\n**Status**: Failed")


@bot.command(name="show")
async def cmd_show(ctx, task_id: str = None):
    """Show one task with parsed payload and artifact paths."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    tasks = read_tasks(tasks_path)
    if task_id:
        target = next((t for t in tasks if t.get("task_id") == task_id), None)
        if not target:
            await ctx.send(f"⚠️ Task `{task_id}` 를 찾을 수 없습니다.")
            return
    elif len(tasks) == 1:
        target = tasks[0]
    else:
        await ctx.send("사용법: `!show <task_id>`")
        return

    meta = target.get("meta", {})
    payload = _payload_data(target)
    artifacts = _artifact_paths(project, target)
    artifact_text = (
        "\n".join(f"- `{path}`" for path in artifacts)
        if artifacts
        else "- (없음 또는 아직 생성 전)"
    )
    request = payload.get("request") or payload.get("raw") or target.get("payload", "")
    repo_path = payload.get("repo_path") or (project or {}).get("repo_path", "?")
    task_type = target.get("task_type", "?")
    links = []
    for key in (
        "github_issue",
        "github_issue_url",
        "issue_url",
        "github_pr",
        "github_pr_url",
        "pr_url",
    ):
        if payload.get(key):
            links.append(f"- {key}: {payload[key]}")
    link_text = "\n".join(links) if links else "- (없음)"

    await ctx.send(
        f"📄 **Task 상세**\n"
        f"**ID**: `{target.get('task_id', '?')}`\n"
        f"**Status**: {ICON_MAP.get(meta.get('status'), '❓')} {meta.get('status', '?')}\n"
        f"**Type**: `{task_type}`\n"
        f"**Created**: {meta.get('created_at', '?')}\n"
        f"**Updated**: {meta.get('updated_at', '?')}\n"
        f"**Repo**: `{repo_path}`\n"
        f"**Request**: {request}\n"
        f"**GitHub**:\n{link_text}\n"
        f"**Artifacts**:\n{artifact_text}"
    )


@bot.command(name="status")
async def cmd_status(ctx):
    """Show tasks for current project category."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    category_id, project, tasks_path = get_category_context(ctx)

    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    tasks = read_tasks(tasks_path)
    if not tasks:
        await ctx.send(f"📋 **{project['name']}** — 태스크 없음")
        return

    lines = [f"📋 **{project['name']} Task Queue**"]
    for t in tasks:
        status = t.get("meta", {}).get("status", "Unknown")
        task_id = t.get("task_id", "?")
        try:
            payload_data = json.loads(t.get("payload", "{}"))
            preview = payload_data.get("request", t.get("payload", ""))
        except (json.JSONDecodeError, TypeError):
            preview = t.get("payload", "")
        preview = preview[:60] + "…" if len(preview) > 60 else preview
        icon = ICON_MAP.get(status, "❓")
        lines.append(f"{icon} `{task_id}` [{status}] — {preview}")
    await ctx.send("\n".join(lines))


@bot.command(name="help")
async def cmd_help(ctx):
    """Show all available commands."""
    await ctx.send(
        "**📖 Stellaris AI Pipeline — 명령어 목록**\n\n"

        "**🗂️ 프로젝트 관리**\n"
        "`!new-project <이름> <경로>` — 신규 프로젝트 생성\n"
        "ㄴ 디렉토리 생성 + git init + Discord 카테고리/채널 4개 자동 생성\n"
        "`!register <경로>` — 현재 카테고리에 기존 Git 레포 등록\n\n"

        "**🤖 AI 작업 실행** *(#planning / #development / #review 채널에서 사용)*\n"
        "`!run <요청>` — AI 작업 시작\n"
        "ㄴ `#planning` → 플래너만 실행\n"
        "ㄴ `#development` → 전체 파이프라인 (Plan+Code+Review)\n"
        "ㄴ `#review` → 리뷰어만 실행\n\n"

        "**✅ 작업 승인/거절**\n"
        "`!approve [task_id]` — 작업 승인 → Processed\n"
        "`!reject [task_id]` — 작업 거절 → Failed\n"
        "`!propose-approve [task_id]` — 후보 승인 → Pending\n"
        "`!propose-reject [task_id]` — 후보 거절 → Failed\n"
        "`!cancel [task_id]` — 완료 전 태스크 취소 → Failed\n\n"

        "**📋 상태 확인**\n"
        "`!status` — 현재 프로젝트 태스크 목록\n"
        "`!show <task_id>` — 태스크 상세 + artifact 경로\n"
        "`!help` — 이 메시지"
    )


if __name__ == "__main__":
    if not DISCORD_BOT_TOKEN:
        raise RuntimeError("DISCORD_BOT_TOKEN environment variable is not set.")
    bot.run(DISCORD_BOT_TOKEN)
