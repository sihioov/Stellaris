"""Payload and presentation helpers for the Stellaris Discord bot."""
import json
import os
from datetime import datetime, timezone
from urllib.parse import quote

from config import (
    CANOPUS_GITHUB_PROJECT_MODE,
    CANOPUS_STATE_PATH,
    GITHUB_OWNER,
    GITHUB_PROJECT_ID,
    GITHUB_PROJECT_NUMBER,
    GITHUB_PROJECT_OWNER,
    GITHUB_PROJECT_OWNER_KIND,
    GITHUB_PROJECT_STATUS_FIELD_ID,
    GITHUB_PROJECT_STATUS_FIELD_NAME,
    GITHUB_PROJECT_STATUS_OPTION_ID,
    GITHUB_PROJECT_STATUS_OPTION_NAME,
    GITHUB_PROJECT_URL,
    GITHUB_REPO,
    NON_MUTATING_GITHUB_PROJECT_MODES,
)
from v1_job_metadata import build_v1_job_metadata


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def truncate_text(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    return text[: max(0, limit - 20)].rstrip() + "\n…(truncated)"


_RUN_IDENTITY_DASH_CHARS = frozenset("-_./: ")


def _sanitize_run_identity(raw: str) -> str:
    """Mirror canopus' apps/canopus/src/core/run_identity.rs ``sanitize_run_identity``.

    Lowercase ASCII alphanumerics survive verbatim; one of ``-_./: `` collapses
    into a single dash; everything else is dropped. Trailing dashes are trimmed.
    Caller must guarantee the result is non-empty for downstream lookup.
    """
    out: list[str] = []
    last_was_dash = False
    for ch in raw.strip():
        if ch.isascii() and ch.isalnum():
            out.append(ch.lower())
            last_was_dash = False
        elif ch in _RUN_IDENTITY_DASH_CHARS:
            if out and not last_was_dash:
                out.append("-")
                last_was_dash = True
    while out and out[-1] == "-":
        out.pop()
    return "".join(out)


def deterministic_agenda_id_for_github_issue(owner: str, repo: str, number) -> str:
    """Build the same ``gh-{owner}-{repo}-{number}`` agenda id Rust ``Agenda::from_github_issue`` produces.

    Raises ``ValueError`` when the inputs sanitize to an empty id (e.g. all
    non-ASCII characters), matching the Rust contract that an agenda id must
    contain at least one alphanumeric.
    """
    raw = f"gh-{owner}-{repo}-{number}"
    sanitised = _sanitize_run_identity(raw)
    if not sanitised:
        raise ValueError("agenda id must contain at least one ASCII letter or digit")
    return sanitised


def _coerce_issue_number(value) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def github_issue_identity(*sources) -> tuple[str, str, int] | None:
    """Pick the first (owner, repo, issue_number) triple from any source dict.

    Falls back through ``GITHUB_OWNER`` / ``GITHUB_REPO`` env defaults for the
    owner/repo half, but the issue number must come from a source dict because
    the env config has no notion of a single canonical Issue.
    """
    owner = None
    repo = None
    number = None
    for src in sources:
        if not isinstance(src, dict):
            continue
        if owner is None:
            owner = src.get("github_owner") or None
        if repo is None:
            repo = src.get("github_repo") or None
        if number is None:
            number = _coerce_issue_number(src.get("github_issue_number"))
    if owner is None:
        owner = GITHUB_OWNER or None
    if repo is None:
        repo = GITHUB_REPO or None
    if owner and repo and number is not None:
        return owner, repo, number
    return None


def resolve_agenda_id(task_id: str, *sources) -> str:
    """Pick a deterministic GitHub-Issue-derived agenda id when available; otherwise the task-id form.

    The deterministic branch is what V2 will lean on so re-processing the same
    Issue stays idempotent. Without a full Issue identity, callers keep the
    pre-existing ``agenda-{task_id}`` id so Europa workflows that have no
    external ledger continue to work.
    """
    identity = github_issue_identity(*sources)
    if identity is not None:
        owner, repo, number = identity
        return deterministic_agenda_id_for_github_issue(owner, repo, number)
    return f"agenda-{task_id}"


def github_repo_slug() -> str | None:
    if GITHUB_OWNER and GITHUB_REPO:
        return f"{GITHUB_OWNER}/{GITHUB_REPO}"
    return None


def github_issue_create_url(title: str) -> str | None:
    slug = github_repo_slug()
    if not slug:
        return None
    return f"https://github.com/{slug}/issues/new?title={quote(title)}"


def canopus_github_project_mode_metadata() -> str | None:
    """Return only non-mutating Project modes for Discord-originated task metadata."""
    mode = CANOPUS_GITHUB_PROJECT_MODE
    if mode in NON_MUTATING_GITHUB_PROJECT_MODES:
        return mode
    return None


GITHUB_INTAKE_PAYLOAD_KEYS = frozenset(
    {
        "github_owner",
        "github_repo",
        "github_repo_slug",
        "github_issue_number",
        "github_issue_url",
        "github_issue_create_url",
        "github_project_id",
        "github_project_url",
        "github_project_item_id",
        "github_project_status",
        "github_project_owner_kind",
        "github_project_owner",
        "github_project_number",
        "github_project_status_field_id",
        "github_project_status_field_name",
        "github_project_status_option_id",
        "github_project_status_option_name",
    }
)


def github_intake_payload_data(intake: dict) -> dict:
    return {
        key: value
        for key, value in intake.items()
        if key in GITHUB_INTAKE_PAYLOAD_KEYS and value is not None
    }


def build_discord_message_link(ctx) -> str | None:
    if not getattr(ctx, "guild", None):
        return None
    return f"https://discord.com/channels/{ctx.guild.id}/{ctx.channel.id}/{ctx.message.id}"


def _id_str(value) -> str | None:
    if value is None:
        return None
    return str(value)


def _discord_thread_id(ctx) -> str | None:
    channel = getattr(ctx, "channel", None)
    if not channel:
        return None
    if getattr(channel, "parent_id", None) is not None or getattr(channel, "parent", None) is not None:
        return _id_str(getattr(channel, "id", None))
    return None


def build_discord_context_metadata(ctx, task_thread=None) -> dict:
    guild_id = _id_str(getattr(getattr(ctx, "guild", None), "id", None))
    channel = getattr(ctx, "channel", None)
    message = getattr(ctx, "message", None)
    channel_id = _id_str(getattr(channel, "id", None))
    message_id = _id_str(getattr(message, "id", None))
    if task_thread is not None:
        thread_id = _id_str(getattr(task_thread, "id", None))
        parent_channel_id = _id_str(getattr(task_thread, "parent_id", None))
        if parent_channel_id is None:
            parent_channel_id = _id_str(getattr(getattr(task_thread, "parent", None), "id", None))
        if parent_channel_id is None:
            parent_channel_id = channel_id
    else:
        thread_id = _discord_thread_id(ctx)
        parent_channel_id = _id_str(getattr(channel, "parent_id", None))
        if parent_channel_id is None:
            parent_channel_id = _id_str(getattr(getattr(channel, "parent", None), "id", None))
        if parent_channel_id is None:
            parent_channel_id = channel_id

    context_kind = "thread" if thread_id else "message"
    if thread_id:
        context_id = f"discord-thread-{thread_id}"
    else:
        context_id = f"discord-message-{guild_id or 'dm'}-{channel_id or 'unknown'}-{message_id or 'unknown'}"

    return {
        "discord_thread_id": thread_id,
        "discord_parent_channel_id": parent_channel_id,
        "discord_context_kind": context_kind,
        "discord_context_id": context_id,
    }


def build_follow_up_attribution(ctx) -> dict:
    return {
        "follow_up_source": "discord",
        "follow_up_user_id": _id_str(getattr(getattr(ctx, "author", None), "id", None)),
        "follow_up_channel_id": _id_str(getattr(getattr(ctx, "channel", None), "id", None)),
        "follow_up_message_id": _id_str(getattr(getattr(ctx, "message", None), "id", None)),
        "follow_up_message_url": build_discord_message_link(ctx),
    }


def canopus_state_root_for_project(project: dict) -> str | None:
    if CANOPUS_STATE_PATH:
        return CANOPUS_STATE_PATH
    repo_path = project.get("repo_path")
    if repo_path:
        return os.path.join(repo_path, ".canopus")
    return None


def canopus_run_id_for_task(agenda_id: str, task_id: str) -> str:
    return _sanitize_run_identity(f"{agenda_id}-{task_id}")


def _job_metadata(task_id: str, agenda_id: str, request: str, project: dict) -> dict:
    state_root = canopus_state_root_for_project(project)
    run_id = canopus_run_id_for_task(agenda_id, task_id)
    return build_v1_job_metadata(
        task_id=task_id,
        agenda_id=agenda_id,
        run_id=run_id,
        request=request,
        project=project,
        state_root=state_root,
        github_project_mode=canopus_github_project_mode_metadata(),
    )


def build_task_payload(
    ctx,
    task_id: str,
    request: str,
    project: dict,
    channel_type: str,
    work_intake: dict | None = None,
    task_thread=None,
) -> dict:
    agenda_id = resolve_agenda_id(task_id, work_intake, project)
    title = truncate_text(request.replace("\n", " "), 90)
    payload = {
        "request": request,
        "repo_path": project["repo_path"],
        "task_id": task_id,
        "agenda_id": agenda_id,
        "canopus_agenda_id": agenda_id,
        "role_mode": {
            "canopus.planner": "plan",
            "canopus.agent": "full",
            "canopus.reviewer": "review",
        }.get(channel_type, "full"),
        "github_owner": project.get("github_owner") or GITHUB_OWNER or None,
        "github_repo": project.get("github_repo") or GITHUB_REPO or None,
        "github_repo_slug": github_repo_slug(),
        "github_issue_number": None,
        "github_issue_url": None,
        "github_issue_create_url": github_issue_create_url(title),
        "github_project_id": project.get("github_project_id") or GITHUB_PROJECT_ID or None,
        "github_project_url": project.get("github_project_url") or GITHUB_PROJECT_URL or None,
        "github_project_item_id": None,
        "github_project_status": "Pending",
        "github_project_owner_kind": project.get("github_project_owner_kind") or GITHUB_PROJECT_OWNER_KIND or None,
        "github_project_owner": project.get("github_project_owner") or GITHUB_PROJECT_OWNER or None,
        "github_project_number": str(project.get("github_project_number") or GITHUB_PROJECT_NUMBER) if (project.get("github_project_number") or GITHUB_PROJECT_NUMBER) else None,
        "github_project_status_field_id": GITHUB_PROJECT_STATUS_FIELD_ID or None,
        "github_project_status_field_name": GITHUB_PROJECT_STATUS_FIELD_NAME or None,
        "github_project_status_option_id": GITHUB_PROJECT_STATUS_OPTION_ID or None,
        "github_project_status_option_name": GITHUB_PROJECT_STATUS_OPTION_NAME or None,
        "github_project_mode": canopus_github_project_mode_metadata(),
        "discord_channel_id": str(ctx.channel.id),
        "discord_message_id": str(ctx.message.id),
        "discord_message_url": build_discord_message_link(ctx),
        "confirmation_state": "requested",
        "approval_state": "pending",
        "approved_at": None,
        "rejected_at": None,
        "finalize_requested_at": None,
    }
    payload.update(build_discord_context_metadata(ctx, task_thread=task_thread))
    payload.update(build_follow_up_attribution(ctx))
    payload.update(_job_metadata(task_id, agenda_id, request, project))
    if work_intake:
        payload.update(github_intake_payload_data(work_intake))
    return {k: v for k, v in payload.items() if v is not None}


def _payload_data(task: dict) -> dict:
    payload = task.get("payload", "")
    if isinstance(payload, dict):
        return payload
    try:
        parsed = json.loads(payload)
        return parsed if isinstance(parsed, dict) else {"raw": payload}
    except (json.JSONDecodeError, TypeError):
        if isinstance(payload, str):
            kv = {}
            for line in payload.splitlines():
                if "=" not in line:
                    continue
                key, value = line.split("=", 1)
                key = key.strip()
                if key:
                    kv[key] = value.strip()
            if kv:
                kv["raw"] = payload
                return kv
        return {"raw": payload}


def _artifact_lookup_ids(task: dict, payload: dict) -> list[str]:
    ids = []

    def add(value) -> None:
        if value is None:
            return
        value = str(value).strip()
        if value and value not in ids:
            ids.append(value)

    add(task.get("task_id"))
    for key in (
        "agenda_id",
        "canopus_agenda_id",
        "run_id",
        "artifact_task_id",
        "backend_id",
        "task_id",
    ):
        add(payload.get(key))
    return ids


def _artifact_paths(project: dict | None, task: dict) -> list[str]:
    payload = _payload_data(task)
    candidates = []
    state_root = CANOPUS_STATE_PATH
    if not state_root and project:
        state_root = canopus_state_root_for_project(project)
    if state_root:
        for lookup_id in _artifact_lookup_ids(task, payload):
            for path in (
                os.path.join(state_root, "artifacts", lookup_id),
                os.path.join(state_root, "runs", f"{lookup_id}.json"),
                os.path.join(state_root, "runs", f"{lookup_id}-finalize.txt"),
                os.path.join(state_root, "runs", f"{lookup_id}-delivery-gate.json"),
            ):
                if _path_under_root(path, state_root):
                    candidates.append(path)
    payload_artifact_paths = payload.get("artifact_paths")
    if isinstance(payload_artifact_paths, dict):
        for path in payload_artifact_paths.values():
            if (
                isinstance(path, str)
                and _path_under_root(path, state_root)
                and path not in candidates
            ):
                candidates.append(path)
    found = []
    for path in candidates:
        if os.path.isdir(path):
            for root, _, files in os.walk(path):
                for name in files:
                    found_path = os.path.join(root, name)
                    if _path_under_root(found_path, state_root) and found_path not in found:
                        found.append(found_path)
        elif os.path.exists(path) and _path_under_root(path, state_root) and path not in found:
            found.append(path)
    return found[:10]


def _path_under_root(path: str, root: str | None) -> bool:
    if not root:
        return False
    try:
        real_root = os.path.realpath(root)
        return os.path.commonpath([os.path.realpath(path), real_root]) == real_root
    except ValueError:
        return False


def mark_task_approved(task: dict, approved_by: str | None = None, approval_source: str | None = None, approval_message_url: str | None = None) -> None:
    ts = now_iso()
    payload = _payload_data(task)
    payload["confirmation_state"] = "approved"
    payload["approval_state"] = "approved"
    payload["approved_at"] = ts
    payload["finalize_requested_at"] = ts
    payload["github_project_status"] = "Approved"
    if approved_by is not None:
        payload["approved_by"] = str(approved_by)
    if approval_source is not None:
        payload["approval_source"] = approval_source
    if approval_message_url is not None:
        payload["approval_message_url"] = approval_message_url
    task["payload"] = json.dumps(payload, ensure_ascii=False)
    task.setdefault("meta", {})["confirmation_state"] = "approved"
    task["meta"]["approval_state"] = "approved"
    task["meta"]["approved_at"] = ts
    task["meta"]["finalize_requested_at"] = ts
    if approved_by is not None:
        task["meta"]["approved_by"] = str(approved_by)
    if approval_source is not None:
        task["meta"]["approval_source"] = approval_source
    if approval_message_url is not None:
        task["meta"]["approval_message_url"] = approval_message_url


def mark_task_rejected(task: dict) -> None:
    ts = now_iso()
    payload = _payload_data(task)
    payload["confirmation_state"] = "rejected"
    payload["approval_state"] = "rejected"
    payload["rejected_at"] = ts
    payload["github_project_status"] = "Rejected"
    task["payload"] = json.dumps(payload, ensure_ascii=False)
    task.setdefault("meta", {})["confirmation_state"] = "rejected"
    task["meta"]["approval_state"] = "rejected"
    task["meta"]["rejected_at"] = ts
