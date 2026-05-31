"""
Discord bot for Stellaris AI pipeline.

Project structure:
  Discord Category = Git project (repo)
  Channels per category:
    #general     - status, !register, !new-project
    #planning    - Planner agent only
    #development - Full pipeline (Planner+Coder+Reviewer)
    #review      - Reviewer agent only
    #analysis    - Analyst-style direct answers via ASK_COMMAND
    #brainstorming - Brainstormer-style direct answers via ASK_COMMAND

Commands:
  !new-project <name> [path]  - Create category + 6 channels + register
  !register <path>            - Register current category's repo path
  !ask <question>             - Ask a direct question without creating a task
  !analyze <topic>            - Ask an analyst-style project-local question
  !brainstorm <topic>         - Ask for project-local brainstorming
  !run <request>              - Add task (agent type from channel name)
  !approve [task_id]          - Approve PendingReview task and request finalization
  !finalize <task_id>         - Retry Canopus finalization for an approved Processed task
  !reject  [task_id]          - Reject  PendingReview task
  !propose-approve [task_id]  - Promote PendingProposal task to Pending
  !propose-reject  [task_id]  - Reject PendingProposal task as Failed
  !cancel [task_id]           - Cancel a non-terminal task as Failed
  !show [task_id]             - Show task details and artifact paths
  !status                     - Show tasks for current project
  !worktree [list|create|switch] - Show/list/create/switch project worktrees

Authorization: set ALLOWED_USER_IDS=123456789,987654321 in env to restrict access.
"""
import json
import os
import subprocess
import uuid

import discord
from discord.ext import commands

import projects_store as _projects_store

from canopus_client import (
    ASK_COMMAND,
    CANOPUS_COMMAND,
    CANOPUS_STATE_PATH,
    create_worktree,
    finalize_approved_task,
    intake_github_work,
    mark_proposal_intake_failed,
    promote_pending_proposal_with_intake,
    register_github_project,
    run_ask_backend,
    run_canopus_json,
)
from config import (
    ALLOWED_USER_IDS,
    ASK_MAX_OUTPUT_CHARS,
    ASK_TIMEOUT_SECONDS,
    CHANNEL_TYPE_MAP,
    DISCORD_BOT_TOKEN,
    ICON_MAP,
    NEW_PROJECT_DEFAULT_ROOT,
    NON_MUTATING_GITHUB_PROJECT_MODES,
    env_int,
    get_channel_type,
    is_authorized,
)
from instruction_router import (
    CI_REPAIR,
    CODE_CHANGE,
    NEEDS_CLARIFICATION,
    PR_REVIEW,
    READ_ONLY_ANALYSIS,
    classify_instruction,
)
from payloads import (
    _artifact_lookup_ids,
    _artifact_paths,
    _payload_data,
    build_discord_message_link,
    build_task_payload,
    canopus_github_project_mode_metadata,
    deterministic_agenda_id_for_github_issue,
    github_issue_create_url,
    github_repo_slug,
    mark_task_approved,
    mark_task_rejected,
    now_iso,
    resolve_agenda_id,
    truncate_text,
)
from projects_store import (
    get_project,
    get_tasks_path,
    merge_project_registration,
    normalize_project_worktrees,
    parse_github_registration_flags,
    read_projects,
    record_project_worktree,
    switch_project_worktree,
    validate_worktree_name,
    write_projects,
)
from tasks_store import (
    _read_tasks_unlocked,
    _write_tasks_unlocked,
    append_task_locked,
    find_single_task_locked,
    lock_path_for,
    read_tasks,
    task_file_lock,
    update_task_status_locked,
    write_tasks,
)


intents = discord.Intents.default()
intents.message_content = True
intents.guilds = True

bot = commands.Bot(command_prefix="!", intents=intents, help_command=None)

PROJECT_CHANNELS = (
    "general",
    "planning",
    "development",
    "review",
    "analysis",
    "brainstorming",
)

ANALYST_STYLE_INSTRUCTION = (
    "Act as the Canopus analyst. Analyze the current request and repository context. "
    "Do not edit files."
)

BRAINSTORMER_STYLE_INSTRUCTION = (
    "Act as the Canopus brainstormer. Suggest 3–5 ideas, options, or perspectives "
    "for the following. Do not commit to any single answer; explore variety. "
    "Do not edit files."
)


# ── helpers ──────────────────────────────────────────────────────────────────

def get_category_context(ctx):
    """Returns (category_id, project, tasks_path) or (None, None, None) if not in a project channel."""
    if not ctx.guild or not hasattr(ctx.channel, "category") or not ctx.channel.category:
        return None, None, None
    category_id = ctx.channel.category.id
    project = get_project(category_id)
    tasks_path = get_tasks_path(category_id)
    return category_id, project, tasks_path


def normalized_channel_name(ctx) -> str:
    return getattr(ctx.channel, "name", "").lower().strip()


def build_conversation_prompt(
    project: dict,
    channel_name: str,
    role_surface: str,
    role_instruction: str,
    topic: str,
) -> str:
    return (
        "Project context:\n"
        f"- project: {project.get('name', '?')}\n"
        f"- repo_path: {project.get('repo_path', '?')}\n"
        f"- channel: #{channel_name}\n"
        f"- role_surface: {role_surface}\n\n"
        "Role instruction:\n"
        f"{role_instruction}\n\n"
        "User request:\n"
        f"{topic}"
    )


async def run_project_conversation_command(
    ctx,
    *,
    topic: str | None,
    required_channel: str,
    role_surface: str,
    role_instruction: str,
    response_label: str,
    command_label: str,
    conversation_role: str,
):
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not topic:
        await ctx.send(f"사용법: `{command_label} <주제>`")
        return

    channel_name = normalized_channel_name(ctx)
    if channel_name != required_channel:
        await ctx.send(f"⚠️ 이 명령은 `#{required_channel}` 채널에서만 사용할 수 있습니다.")
        return

    category_id, project, _tasks_path = get_category_context(ctx)
    if category_id is None:
        await ctx.send("⚠️ 카테고리가 있는 채널에서만 사용할 수 있습니다.")
        return
    if project is None:
        await ctx.send(
            "⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.\n"
            "`!register <로컬경로>` 또는 `!new-project <이름> [경로]` 를 먼저 실행해주세요."
        )
        return

    prompt = build_conversation_prompt(
        project,
        channel_name,
        role_surface,
        role_instruction,
        topic,
    )
    extra_env = {
        "STELLARIS_PROJECT_NAME": str(project.get("name", "")),
        "STELLARIS_PROJECT_REPO_PATH": str(project.get("repo_path", "")),
        "STELLARIS_DISCORD_CHANNEL": f"#{channel_name}",
        "STELLARIS_CONVERSATION_ROLE": conversation_role,
    }

    async with ctx.typing():
        answer, error = await run_ask_backend(
            prompt,
            cwd=project.get("repo_path"),
            extra_env=extra_env,
            command_label=command_label,
        )

    if error:
        await ctx.send(error)
        return

    await ctx.send(f"💬 **{response_label}**\n{answer}")


def is_approved_processed_task(task: dict | None) -> bool:
    if not task:
        return False
    meta = task.get("meta", {})
    payload = _payload_data(task)
    return meta.get("status") == "Processed" and (
        meta.get("approval_state") == "approved" or payload.get("approval_state") == "approved"
    )


def approve_error_with_finalize_hint(tasks_path: str, task_id: str | None, error: str) -> str:
    if not task_id:
        return error
    target = next((t for t in read_tasks(tasks_path) if t.get("task_id") == task_id), None)
    if is_approved_processed_task(target):
        return f"{error}\n이미 승인된 태스크입니다. 최종화를 다시 시도하려면 `!finalize {task_id}` 를 사용하세요."
    return error


def canopus_error_message(result: dict | None, error: str | None) -> str:
    if isinstance(result, dict):
        err = result.get("error")
        if isinstance(err, dict):
            return str(err.get("message") or err.get("code") or error or "unknown error")
        if err:
            return str(err)
    return str(error or "unknown error")


def finalize_retry_command(task_id: str | None) -> str:
    return f"`!finalize {task_id}`" if task_id else "`!finalize <task_id>`"


def format_token_usage(result: dict | None) -> str:
    if not result:
        return ""
    usage = result.get("token_usage")
    if not isinstance(usage, dict):
        return ""

    input_tokens = int(usage.get("input_tokens") or 0)
    output_tokens = int(usage.get("output_tokens") or 0)
    total_tokens = int(usage.get("total_tokens") or (input_tokens + output_tokens))
    if total_tokens <= 0:
        return ""

    return (
        f"\n**Tokens**: {total_tokens:,} total "
        f"(input {input_tokens / 1000:.1f}k / output {output_tokens / 1000:.1f}k)"
    )


def format_finalize_outcome(result: dict | None, error: str | None, task_id: str | None = None) -> str:
    retry_command = finalize_retry_command(task_id)
    if result is None:
        if error and "CANOPUS_COMMAND is not configured" in error:
            return (
                "**Finalize**: not attempted (CANOPUS_COMMAND is not configured)\n"
                f"`CANOPUS_COMMAND` 설정 후 {retry_command} 로 다시 시도하세요."
            )
        return (
            f"**Finalize**: failed — {truncate_text(error or 'unknown error', 500)}\n"
            f"Retry: {retry_command}"
        )

    if result.get("ok") is False:
        retry_hint = f"Retry: {retry_command}" if result.get("retryable") is not False else f"Retry may require operator action before {retry_command}."
        return (
            f"**Finalize**: failed — {truncate_text(canopus_error_message(result, error), 500)}\n"
            f"{retry_hint}"
        )

    status = result.get("status") or "completed"
    if status == "dry_run":
        summary = "dry-run/gate-disabled; no local commit"
    elif status == "already_finalized":
        summary = "already finalized"
    elif status == "no_changes":
        summary = "no changes"
    elif status == "finalized":
        summary = "finalized"
    else:
        summary = str(status)

    details = []
    if result.get("branch"):
        details.append(f"branch `{result['branch']}`")
    if result.get("commit"):
        details.append(f"commit `{result['commit']}`")
    suffix = f" ({', '.join(details)})" if details else ""
    return f"**Finalize**: {summary}{suffix}{format_token_usage(result)}"


# ── events ───────────────────────────────────────────────────────────────────

@bot.event
async def on_ready():
    print(f"[europa] Logged in as {bot.user} (id={bot.user.id})")
    projects = read_projects()
    count = len(projects["projects"])
    print(f"[europa] {count} project(s) registered in projects.json")
    if ALLOWED_USER_IDS:
        print(f"[europa] Authorized user IDs: {ALLOWED_USER_IDS}")
    else:
        print("[europa] WARNING: ALLOWED_USER_IDS not set — all users can run commands")


def is_git_repo_path(repo_path: str) -> bool:
    """Return true for normal Git repos and linked git worktrees."""
    if not os.path.isdir(repo_path):
        return False
    try:
        result = subprocess.run(
            ["git", "-C", repo_path, "rev-parse", "--is-inside-work-tree"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        if result.returncode == 0 and result.stdout.strip() == "true":
            return True
    except (OSError, subprocess.SubprocessError):
        pass
    return os.path.exists(os.path.join(repo_path, ".git"))


def git_command(repo_path: str, *args: str) -> tuple[int, str, str]:
    try:
        result = subprocess.run(
            ["git", "-C", repo_path, *args],
            capture_output=True,
            text=True,
            timeout=10,
        )
        return result.returncode, result.stdout.strip(), result.stderr.strip()
    except (OSError, subprocess.SubprocessError) as exc:
        return 1, "", str(exc)


def default_new_project_repo_path(name: str) -> str:
    """Return the default repo path for a Discord-created project name."""
    project_name = (name or "").strip()
    if not project_name:
        raise ValueError("프로젝트 이름이 필요합니다.")
    path_separators = {sep for sep in (os.sep, os.altsep) if sep}
    if (
        project_name in {".", ".."}
        or os.path.isabs(project_name)
        or any(sep in project_name for sep in path_separators)
    ):
        raise ValueError(
            f"프로젝트 이름은 기본 경로 아래의 단일 디렉토리 이름이어야 합니다: `{project_name}`"
        )

    root = os.path.abspath(os.path.expanduser(NEW_PROJECT_DEFAULT_ROOT))
    repo_path = os.path.abspath(os.path.join(root, project_name))
    if repo_path == root or os.path.commonpath([root, repo_path]) != root:
        raise ValueError(
            f"프로젝트 이름으로 기본 경로 밖을 가리킬 수 없습니다: `{project_name}`"
        )
    return repo_path


def worktree_status_text(repo_path: str) -> str:
    if not is_git_repo_path(repo_path):
        return f"❌ Git worktree가 아닙니다: `{repo_path}`"

    _, root, _ = git_command(repo_path, "rev-parse", "--show-toplevel")
    _, branch, _ = git_command(repo_path, "branch", "--show-current")
    _, head, _ = git_command(repo_path, "rev-parse", "--short", "HEAD")
    _, git_common_dir, _ = git_command(repo_path, "rev-parse", "--git-common-dir")
    status_code, status, status_err = git_command(repo_path, "status", "--short")

    if status_code != 0:
        dirty_text = f"⚠️ status 확인 실패: {status_err or '(unknown)'}"
    elif not status:
        dirty_text = "✅ clean"
    else:
        lines = status.splitlines()
        preview = "\n".join(lines[:10])
        if len(lines) > 10:
            preview += f"\n… +{len(lines) - 10} more"
        dirty_text = f"⚠️ dirty ({len(lines)} changed)\n```text\n{preview}\n```"

    branch_text = branch or "(detached)"
    return (
        "🧰 **Worktree 상태**\n"
        f"**Repo**: `{root or repo_path}`\n"
        f"**Branch**: `{branch_text}`\n"
        f"**HEAD**: `{head or '?'}`\n"
        f"**Git dir**: `{git_common_dir or '?'}`\n"
        f"**Status**: {dirty_text}"
    )



def worktree_list_text(project: dict) -> str:
    normalized = normalize_project_worktrees(project)
    active = normalized.get("active_worktree")
    lines = ["🧰 **Worktree 목록**"]
    lines.append(f"**Base**: `{normalized.get('base_repo_path', '?')}`")
    lines.append(f"**Active**: `{active}`")
    for name, entry in sorted(normalized.get("worktrees", {}).items()):
        path = entry.get("repo_path", "?") if isinstance(entry, dict) else str(entry)
        marker = "➡️" if name == active else "•"
        validity = "✅" if is_git_repo_path(path) else "⚠️ invalid/missing"
        lines.append(f"{marker} `{name}` — {validity} `{path}`")
    return "\n".join(lines)


def worktree_usage_text() -> str:
    return (
        "사용법:\n"
        "`!worktree` — active worktree 상태\n"
        "`!worktree list` — 등록된 worktree 목록\n"
        "`!worktree create <name>` — Canopus를 통해 새 worktree 생성\n"
        "`!worktree switch <name>` — active worktree 변경"
    )


async def handle_worktree_create(ctx, category_id: int, project: dict, name: str | None) -> None:
    name = (name or "").strip()
    name_error = validate_worktree_name(name)
    if name_error:
        await ctx.send(f"⚠️ {name_error}\n{worktree_usage_text()}")
        return

    data = read_projects()
    current = normalize_project_worktrees(data["projects"].get(str(category_id), project))
    if name in current.get("worktrees", {}):
        await ctx.send(f"⚠️ 이미 등록된 worktree 이름입니다: `{name}`")
        return

    result, error = await create_worktree(current["base_repo_path"], name)
    if error:
        await ctx.send(f"❌ worktree 생성 실패: {truncate_text(error, 800)}")
        return
    if not result or not result.get("repo_path"):
        await ctx.send("❌ worktree 생성 실패: Canopus 응답에 repo_path가 없습니다.")
        return

    updated = record_project_worktree(current, name, result["repo_path"], now_iso())
    data["projects"][str(category_id)] = updated
    write_projects(data)
    await ctx.send(
        "✅ **worktree 생성됨**\n"
        f"**Name**: `{name}`\n"
        f"**Repo**: `{result['repo_path']}`\n"
        "활성화하려면 `!worktree switch {}` 를 실행하세요.".format(name)
    )


async def handle_worktree_switch(ctx, category_id: int, project: dict, name: str | None) -> None:
    name = (name or "").strip()
    name_error = validate_worktree_name(name)
    if name_error:
        await ctx.send(f"⚠️ {name_error}\n{worktree_usage_text()}")
        return

    data = read_projects()
    current = normalize_project_worktrees(data["projects"].get(str(category_id), project))
    updated, error = switch_project_worktree(current, name)
    if error:
        await ctx.send(f"⚠️ {error}\n`!worktree list` 로 확인하세요.")
        return
    repo_path = updated["repo_path"]
    if not is_git_repo_path(repo_path):
        await ctx.send(f"❌ Git worktree가 아닙니다: `{repo_path}`")
        return

    data["projects"][str(category_id)] = updated
    write_projects(data)
    await ctx.send(
        "✅ **active worktree 변경됨**\n"
        f"**Name**: `{name}`\n"
        f"**Repo**: `{repo_path}`\n"
        "이후 생성되는 `!run` task payload는 이 경로를 사용합니다."
    )


# ── commands ──────────────────────────────────────────────────────────────────

@bot.command(name="new-project")
async def cmd_new_project(ctx, name: str = None, *, repo_path: str = None):
    """Create directory, git init, Discord category + 6 channels, register in projects.json."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not name:
        await ctx.send("사용법: `!new-project <이름> [로컬경로] [--github owner/repo --project-owner org:name|user:name --create-github-repo]`")
        return
    repo_path, github_opts, parse_error = parse_github_registration_flags(repo_path or "")
    if parse_error:
        await ctx.send(f"⚠️ {parse_error}")
        return
    if not repo_path:
        try:
            repo_path = default_new_project_repo_path(name)
        except ValueError as exc:
            await ctx.send(f"⚠️ {exc}")
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
        if is_git_repo_path(repo_path):
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
        for ch_name in PROJECT_CHANNELS:
            await category.create_text_channel(ch_name)
        steps.append("💬 Discord 카테고리 + 6채널 생성")

        # 4. GitHub-backed registration (when requested) and projects.json registration.
        registration, reg_error = await register_github_project(repo_path, github_opts)
        if reg_error:
            if category is not None:
                try:
                    await category.delete(reason="GitHub registration failed before project activation")
                    steps.append("↩️ GitHub 등록 실패로 Discord 카테고리 롤백")
                except Exception:
                    steps.append("⚠️ GitHub 등록 실패; 생성된 Discord 카테고리는 미등록/provisional 상태")
            await ctx.send(f"❌ GitHub 등록 실패: {reg_error}")
            return
        data = read_projects()
        record = {
            "name": name,
            "repo_path": repo_path,
            "registered_at": now_iso(),
        }
        if registration:
            record = merge_project_registration(record, registration)
        data["projects"][str(category.id)] = record
        write_projects(data)
        steps.append("📝 projects.json 등록 완료")

        step_list = "\n".join(steps)
        await ctx.send(
            f"✅ **프로젝트 생성 완료**: {name}\n\n"
            f"{step_list}\n\n"
            f"`#analysis`/`#brainstorming`에서는 대화형 질문을, "
            f"`#development`에서는 `!run <요청>` 으로 작업을 시작하세요."
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
        await ctx.send("사용법: `!register <로컬경로> [--github owner/repo --project-owner org:name|user:name]`")
        return
    repo_path, github_opts, parse_error = parse_github_registration_flags(repo_path)
    if parse_error:
        await ctx.send(f"⚠️ {parse_error}")
        return
    if not os.path.isdir(repo_path):
        await ctx.send(f"❌ 경로가 존재하지 않습니다: `{repo_path}`\n신규 프로젝트라면 `!new-project <이름> [경로]` 를 사용하세요.")
        return
    if not is_git_repo_path(repo_path):
        await ctx.send(f"❌ Git 레포지토리가 아닙니다: `{repo_path}`\n`git init`을 먼저 실행하거나 `!new-project` 를 사용하세요.")
        return
    if not ctx.guild or not hasattr(ctx.channel, "category") or not ctx.channel.category:
        await ctx.send("⚠️ 카테고리가 있는 채널에서만 사용할 수 있습니다.")
        return

    category = ctx.channel.category
    data = read_projects()
    is_update = str(category.id) in data["projects"]
    record = {
        "name": category.name,
        "repo_path": repo_path,
        "registered_at": now_iso(),
    }
    registration, reg_error = await register_github_project(repo_path, github_opts)
    if reg_error:
        await ctx.send(f"❌ GitHub 등록 실패: {reg_error}")
        return
    if registration:
        record = merge_project_registration(record, registration)
    data["projects"][str(category.id)] = record
    write_projects(data)

    action = "업데이트됨" if is_update else "등록됨"
    await ctx.send(
        f"✅ **프로젝트 {action}**\n"
        f"📁 카테고리: **{category.name}**\n"
        f"📂 Repo: `{repo_path}`"
    )


@bot.command(name="ask")
async def cmd_ask(ctx, *, question: str = None):
    """Ask a direct question without creating a pipeline task."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return
    if not question:
        await ctx.send("사용법: `!ask <질문>`\n예: `!ask 오늘 날씨 어때?`")
        return

    async with ctx.typing():
        answer, error = await run_ask_backend(question)

    if error:
        await ctx.send(error)
        return

    await ctx.send(f"💬 **Ask**\n{answer}")


@bot.command(name="analyze")
async def cmd_analyze(ctx, *, topic: str = None):
    """Ask an analyst-style project-local question without creating a task."""
    await run_project_conversation_command(
        ctx,
        topic=topic,
        required_channel="analysis",
        role_surface="analyst-style",
        role_instruction=ANALYST_STYLE_INSTRUCTION,
        response_label="Analyze",
        command_label="!analyze",
        conversation_role="analysis",
    )


@bot.command(name="brainstorm")
async def cmd_brainstorm(ctx, *, topic: str = None):
    """Ask for project-local brainstorming without creating a task."""
    await run_project_conversation_command(
        ctx,
        topic=topic,
        required_channel="brainstorming",
        role_surface="brainstormer-style",
        role_instruction=BRAINSTORMER_STYLE_INSTRUCTION,
        response_label="Brainstorm",
        command_label="!brainstorm",
        conversation_role="brainstorming",
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
            f"`!register <로컬경로>` 또는 `!new-project <이름> [경로]` 를 먼저 실행해주세요."
        )
        return

    channel_type = get_channel_type(ctx.channel.name)
    if channel_type is None:
        channel_name = normalized_channel_name(ctx)
        if channel_name == "analysis":
            await ctx.send("⚠️ 이 채널에서는 `!analyze <주제>` 명령을 사용하세요. `!run`은 `#planning`, `#development`, `#review` 채널에서만 작동합니다.")
            return
        if channel_name == "brainstorming":
            await ctx.send("⚠️ 이 채널에서는 `!brainstorm <주제>` 명령을 사용하세요. `!run`은 `#planning`, `#development`, `#review` 채널에서만 작동합니다.")
            return
        await ctx.send("⚠️ `#planning`, `#development`, `#review` 채널에서만 작업을 실행할 수 있습니다.")
        return

    task_id = f"discord-{uuid.uuid4().hex[:12]}"
    ts = now_iso()
    agenda_id = f"agenda-{task_id}"
    work_intake, intake_error = await intake_github_work(project, task_id, agenda_id, request, build_discord_message_link(ctx))
    if intake_error:
        if os.environ.get("EUROPA_INTAKE_FAILURE_LOCAL_ONLY", "0") == "1":
            await ctx.send(f"⚠️ GitHub intake 실패, 로컬로만 진행합니다: {intake_error}")
            work_intake = None
        else:
            await ctx.send(f"❌ GitHub work-intake 실패: {intake_error}")
            return
    payload = build_task_payload(ctx, task_id, request, project, channel_type, work_intake)
    task = {
        "task_id": task_id,
        "task_type": {"Custom": channel_type},
        "payload": json.dumps(payload, ensure_ascii=False),
        "meta": {
            "status": "Pending",
            "created_at": ts,
            "updated_at": ts,
            "agenda_id": payload["agenda_id"],
            "job_id": payload["job_id"],
            "job_status": payload["job_status"],
            "intent": payload["intent"],
            "classification_reason": payload["classification_reason"],
            "confirmation_state": payload["confirmation_state"],
            "approval_state": payload["approval_state"],
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
        f"**Agenda**: `{payload['agenda_id']}`\n"
        f"**GitHub Issue**: {payload.get('github_issue_url') or payload.get('github_issue_create_url') or '(not configured)'}\n"
        f"**Project**: {payload.get('github_project_url') or payload.get('github_project_id') or '(not configured)'}\n"
        f"**Status**: Pending"
    )



@bot.command(name="approve")
async def cmd_approve(ctx, task_id: str = None):
    """Approve a PendingReview task → Processed, then ask Canopus to finalize it."""
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
        "approve",
        {"PendingReview"},
        "PendingReview",
        "Processed",
        lambda task: mark_task_approved(
            task,
            approved_by=str(ctx.author.id),
            approval_source="discord",
            approval_message_url=build_discord_message_link(ctx),
        ),
    )
    if error:
        await ctx.send(approve_error_with_finalize_hint(tasks_path, task_id, error))
        return

    payload = _payload_data(target)
    finalize_result, finalize_error = await finalize_approved_task(tasks_path, target["task_id"])
    await ctx.send(
        f"✅ 태스크 승인됨\n"
        f"**ID**: `{target['task_id']}`\n"
        f"**Agenda**: `{payload.get('agenda_id', '?')}`\n"
        f"**Status**: Processed\n"
        f"{format_finalize_outcome(finalize_result, finalize_error, target['task_id'])}"
    )


@bot.command(name="finalize")
async def cmd_finalize(ctx, task_id: str = None):
    """Retry Canopus finalization for an already-approved Processed task."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    if not task_id:
        await ctx.send("사용법: `!finalize <task_id>`")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    target, error = find_single_task_locked(
        tasks_path,
        task_id,
        "finalize",
        {"Processed"},
        "승인 완료(Processed)",
    )
    if error:
        await ctx.send(error)
        return
    if not is_approved_processed_task(target):
        await ctx.send(f"⚠️ Task `{task_id}` 는 승인된 Processed 태스크가 아닙니다.")
        return

    result, finalize_error = await finalize_approved_task(tasks_path, target["task_id"])
    payload = _payload_data(target)
    await ctx.send(
        f"🔁 최종화 재시도\n"
        f"**ID**: `{target['task_id']}`\n"
        f"**Agenda**: `{payload.get('agenda_id', '?')}`\n"
        f"{format_finalize_outcome(result, finalize_error, target['task_id'])}"
    )


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
        tasks_path, task_id, "reject", {"PendingReview"}, "PendingReview", "Failed", mark_task_rejected
    )
    if error:
        await ctx.send(error)
        return

    payload = _payload_data(target)
    await ctx.send(
        f"❌ 태스크 거부됨\n"
        f"**ID**: `{target['task_id']}`\n"
        f"**Agenda**: `{payload.get('agenda_id', '?')}`\n"
        f"**Status**: Failed\n"
        f"**Finalize**: blocked"
    )


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

    proposal, error = find_single_task_locked(tasks_path, task_id, "propose-approve", {"PendingProposal"}, "PendingProposal")
    if error:
        await ctx.send(error)
        return
    payload = _payload_data(proposal)
    request = payload.get("request") or payload.get("prompt") or "Approved proposal"
    agenda_id = (
        payload.get("agenda_id")
        or payload.get("canopus_agenda_id")
        or resolve_agenda_id(proposal["task_id"], payload, project)
    )
    work_intake, intake_error = await intake_github_work(project, proposal["task_id"], agenda_id, request, payload.get("discord_message_url"))
    if intake_error:
        if os.environ.get("EUROPA_INTAKE_FAILURE_LOCAL_ONLY", "0") == "1":
            await ctx.send(f"⚠️ GitHub intake 실패, 로컬로만 진행합니다: {intake_error}")
            work_intake = None
        else:
            update_task_status_locked(
                tasks_path,
                proposal["task_id"],
                "propose-approve",
                {"PendingProposal"},
                "PendingProposal",
                "PendingProposal",
                lambda task: mark_proposal_intake_failed(task, intake_error),
            )
            await ctx.send(f"❌ GitHub work-intake 실패: {intake_error}")
            return
    target, error = update_task_status_locked(
        tasks_path,
        proposal["task_id"],
        "propose-approve",
        {"PendingProposal"},
        "PendingProposal",
        "Pending",
        lambda task: promote_pending_proposal_with_intake(task, work_intake),
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
    request = (
        payload.get("request")
        or payload.get("prompt")
        or payload.get("raw")
        or target.get("payload", "")
    )
    repo_path = payload.get("repo_path") or (project or {}).get("repo_path", "?")
    task_type = target.get("task_type", "?")
    links = []
    for key in (
        "agenda_id",
        "discord_channel_id",
        "discord_message_id",
        "github_issue",
        "github_issue_url",
        "github_issue_create_url",
        "issue_url",
        "github_project_id",
        "github_project_url",
        "github_project_item_id",
        "github_project_owner_kind",
        "github_project_owner",
        "github_project_number",
        "github_project_status",
        "github_project_mode",
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



@bot.command(name="worktree")
async def cmd_worktree(ctx, action: str = None, *, name: str = None):
    """Show, list, create, or switch registered repo/worktrees."""
    if not is_authorized(ctx):
        await ctx.send("🚫 권한이 없습니다.")
        return

    category_id, project, tasks_path = get_category_context(ctx)
    if project is None:
        await ctx.send("⚠️ 등록된 프로젝트 채널에서만 사용할 수 있습니다.")
        return

    action = (action or "status").strip().lower()
    if action in {"status", "show"}:
        await ctx.send(worktree_status_text(project.get("repo_path", "")))
        return
    if action == "list":
        await ctx.send(worktree_list_text(project))
        return
    if action == "create":
        await handle_worktree_create(ctx, category_id, project, name)
        return
    if action == "switch":
        await handle_worktree_switch(ctx, category_id, project, name)
        return

    await ctx.send(f"⚠️ 알 수 없는 worktree 명령입니다: `{action}`\n{worktree_usage_text()}")


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
        agenda_id = _payload_data(t).get("agenda_id")
        agenda = f" ({agenda_id})" if agenda_id else ""
        preview = preview[:60] + "…" if len(preview) > 60 else preview
        icon = ICON_MAP.get(status, "❓")
        lines.append(f"{icon} `{task_id}`{agenda} [{status}] — {preview}")
    await ctx.send("\n".join(lines))


@bot.command(name="help")
async def cmd_help(ctx):
    """Show all available commands."""
    await ctx.send(
        "**📖 Stellaris AI Pipeline — 명령어 목록**\n\n"

        "**🗂️ 프로젝트 관리**\n"
        "`!new-project <이름> [경로]` — 신규 프로젝트 생성\n"
        f"ㄴ 경로 생략 시 `{NEW_PROJECT_DEFAULT_ROOT}/<이름>` 사용; 디렉토리 생성 + git init + Discord 카테고리/채널 6개 자동 생성\n"
        "`!register <경로>` — 현재 카테고리에 기존 Git 레포 등록\n"
        "`!worktree` / `!worktree list` — active worktree 상태/목록 확인\n"
        "`!worktree create <name>` — Canopus를 통해 새 worktree 생성\n"
        "`!worktree switch <name>` — 이후 `!run` 대상 worktree 변경\n\n"

        "**🤖 AI 작업 실행** *(#planning / #development / #review 채널에서 사용)*\n"
        "`!run <요청>` — AI 작업 시작\n"
        "ㄴ `#planning` → 플래너만 실행\n"
        "ㄴ `#development` → 전체 파이프라인 (Plan+Code+Review)\n"
        "ㄴ `#review` → 리뷰어만 실행\n\n"

        "**💬 즉답 질문**\n"
        "`!ask <질문>` — task를 만들지 않고 바로 질문\n"
        "ㄴ `ASK_COMMAND` 환경변수로 답변 백엔드 설정 필요\n\n"

        "**🔎 프로젝트 대화 채널** *(등록된 프로젝트 카테고리에서 사용)*\n"
        "`!analyze <주제>` — `#analysis`에서 analyst-style 답변\n"
        "`!brainstorm <주제>` — `#brainstorming`에서 brainstormer-style 아이디어 탐색\n"
        "ㄴ task/agenda 없이 프로젝트 repo를 cwd/env/prompt로 `ASK_COMMAND`에 전달\n\n"

        "**✅ 작업 승인/거절**\n"
        "`!approve [task_id]` — 작업 승인 → Processed 후 Canopus 최종화 요청\n"
        "`!finalize <task_id>` — 승인된 작업 최종화 재시도\n"
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
