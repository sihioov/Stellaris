"""Canopus subprocess and backend helpers for the Stellaris Discord bot."""
import asyncio
import json
import os
import shlex

from config import (
    ASK_COMMAND,
    ASK_MAX_OUTPUT_CHARS,
    ASK_TIMEOUT_SECONDS,
    CANOPUS_COMMAND,
    CANOPUS_STATE_PATH,
)
from payloads import _payload_data, now_iso, truncate_text


async def run_canopus_json(args: list[str]) -> tuple[dict | None, str | None]:
    if not CANOPUS_COMMAND:
        return None, "CANOPUS_COMMAND is not configured"
    try:
        argv = shlex.split(CANOPUS_COMMAND, posix=os.name != "nt") + args
    except ValueError as exc:
        return None, f"CANOPUS_COMMAND 파싱 실패: {exc}"
    proc = await asyncio.create_subprocess_exec(
        *argv,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await proc.communicate()
    out = stdout.decode(errors="replace").strip()
    err = stderr.decode(errors="replace").strip()
    if proc.returncode != 0:
        if out:
            try:
                parsed_error = json.loads(out)
            except json.JSONDecodeError:
                parsed_error = None
            if isinstance(parsed_error, dict) and parsed_error.get("ok") is False:
                return parsed_error, err or parsed_error.get("error") or f"canopus exited {proc.returncode}"
        return None, err or out or f"canopus exited {proc.returncode}"
    try:
        parsed = json.loads(out)
    except json.JSONDecodeError as exc:
        return None, f"Canopus JSON 파싱 실패: {exc}: {out[:200]}"
    if not isinstance(parsed, dict):
        return None, "Canopus 응답이 JSON object가 아닙니다."
    return parsed, None


async def register_github_project(repo_path: str, github_opts: dict | None) -> tuple[dict | None, str | None]:
    if not github_opts:
        return None, None
    args = [
        "project-register",
        "--repo", repo_path,
        "--github-owner", github_opts["github_owner"],
        "--github-repo", github_opts["github_repo"],
        "--project-owner-kind", github_opts["github_project_owner_kind"],
        "--project-owner", github_opts["github_project_owner"],
        "--json",
    ]
    if github_opts.get("create_github_repo"):
        args.append("--create-github-repo")
    return await run_canopus_json(args)


async def intake_github_work(project: dict, task_id: str, agenda_id: str, request: str, message_url: str | None) -> tuple[dict | None, str | None]:
    if not (project.get("github_owner") and project.get("github_repo")):
        return None, None
    args = [
        "work-intake",
        "--repo", project["repo_path"],
        "--registration", json.dumps(project, ensure_ascii=False),
        "--task-id", task_id,
        "--agenda-id", agenda_id,
        "--request", request,
        "--project-sync", "best-effort",
        "--json",
    ]
    if message_url:
        args.extend(["--discord-message-url", message_url])
    return await run_canopus_json(args)


def mark_proposal_intake_failed(task: dict, error: str) -> None:
    payload = _payload_data(task)
    payload["proposal_intake_state"] = "failed"
    payload["proposal_intake_error"] = truncate_text(error, 300)
    payload["proposal_intake_failed_step"] = "work_intake"
    payload["proposal_intake_attempted_at"] = now_iso()
    task["payload"] = json.dumps(payload, ensure_ascii=False)
    task.setdefault("meta", {})["proposal_intake_state"] = "failed"
    task["meta"]["proposal_intake_error"] = payload["proposal_intake_error"]


def promote_pending_proposal_with_intake(task: dict, intake: dict | None) -> None:
    payload = _payload_data(task)
    if intake:
        payload.update({k: v for k, v in intake.items() if k.startswith("github_") and v is not None})
        payload["proposal_intake_state"] = "succeeded"
        payload["proposal_intake_attempted_at"] = now_iso()
    task["payload"] = json.dumps(payload, ensure_ascii=False)
    task.setdefault("meta", {})["proposal_intake_state"] = payload.get("proposal_intake_state", "not_required")


async def run_ask_backend(question: str) -> tuple[str | None, str | None]:
    """Run the configured direct-answer backend without touching the task queue."""
    if not ASK_COMMAND:
        return None, (
            "⚠️ `!ask` 답변 백엔드가 설정되지 않았습니다.\n"
            "`ASK_COMMAND` 환경변수에 질문을 stdin으로 받는 명령을 설정해주세요."
        )

    try:
        argv = shlex.split(ASK_COMMAND, posix=os.name != "nt")
    except ValueError as exc:
        return None, f"⚠️ `ASK_COMMAND` 파싱 실패: {exc}"
    if not argv:
        return None, "⚠️ `ASK_COMMAND`가 비어 있습니다."

    env = os.environ.copy()
    env["STELLARIS_ASK_PROMPT"] = question
    try:
        proc = await asyncio.create_subprocess_exec(
            *argv,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            env=env,
        )
    except FileNotFoundError:
        return None, f"⚠️ `ASK_COMMAND` 실행 파일을 찾을 수 없습니다: `{argv[0]}`"
    except OSError as exc:
        return None, f"⚠️ `ASK_COMMAND` 실행 실패: {exc}"

    try:
        stdout, stderr = await asyncio.wait_for(
            proc.communicate(question.encode("utf-8")),
            timeout=ASK_TIMEOUT_SECONDS,
        )
    except asyncio.TimeoutError:
        proc.kill()
        await proc.communicate()
        return None, f"⏱️ `!ask` 시간이 초과되었습니다 ({ASK_TIMEOUT_SECONDS}s)."

    out = stdout.decode("utf-8", errors="replace").strip()
    err = stderr.decode("utf-8", errors="replace").strip()
    if proc.returncode != 0:
        details = truncate_text(err or out or "no output", 700)
        return None, f"❌ `ASK_COMMAND` 실패(exit {proc.returncode}):\n```text\n{details}\n```"
    if not out:
        return None, "⚠️ `ASK_COMMAND`가 빈 답변을 반환했습니다."
    return truncate_text(out, ASK_MAX_OUTPUT_CHARS), None
