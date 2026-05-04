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


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def truncate_text(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    return text[: max(0, limit - 20)].rstrip() + "\n…(truncated)"


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


def build_discord_message_link(ctx) -> str | None:
    if not getattr(ctx, "guild", None):
        return None
    return f"https://discord.com/channels/{ctx.guild.id}/{ctx.channel.id}/{ctx.message.id}"


def build_task_payload(ctx, task_id: str, request: str, project: dict, channel_type: str, work_intake: dict | None = None) -> dict:
    agenda_id = f"agenda-{task_id}"
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
    if work_intake:
        payload.update({k: v for k, v in work_intake.items() if k.startswith("github_") and v is not None})
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
    state_root = CANOPUS_STATE_PATH
    if not state_root and project:
        state_root = os.path.join(project.get("repo_path", ""), ".canopus")
    if not state_root:
        return []

    payload = _payload_data(task)
    candidates = []
    for lookup_id in _artifact_lookup_ids(task, payload):
        candidates.extend([
            os.path.join(state_root, "artifacts", lookup_id),
            os.path.join(state_root, "runs", f"{lookup_id}.json"),
        ])
    found = []
    for path in candidates:
        if os.path.isdir(path):
            for root, _, files in os.walk(path):
                for name in files:
                    found_path = os.path.join(root, name)
                    if found_path not in found:
                        found.append(found_path)
        elif os.path.exists(path) and path not in found:
            found.append(path)
    return found[:10]


def mark_task_approved(task: dict) -> None:
    ts = now_iso()
    payload = _payload_data(task)
    payload["confirmation_state"] = "approved"
    payload["approval_state"] = "approved"
    payload["approved_at"] = ts
    payload["finalize_requested_at"] = ts
    payload["github_project_status"] = "Approved"
    task["payload"] = json.dumps(payload, ensure_ascii=False)
    task.setdefault("meta", {})["confirmation_state"] = "approved"
    task["meta"]["approval_state"] = "approved"
    task["meta"]["approved_at"] = ts
    task["meta"]["finalize_requested_at"] = ts


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
