"""Project registration persistence helpers for the Stellaris Discord bot."""
import json
import os
import shlex
import tempfile

from config import PROJECTS_JSON_PATH, TASKS_DIR, TASKS_JSON_PATH


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
    if TASKS_JSON_PATH:
        return os.path.abspath(os.path.expanduser(TASKS_JSON_PATH))
    return os.path.join(TASKS_DIR, f"tasks-{category_id}.json")


def parse_github_registration_flags(text: str) -> tuple[str, dict | None, str | None]:
    try:
        parts = shlex.split(text, posix=os.name != "nt")
    except ValueError as exc:
        return text, None, f"GitHub 옵션 파싱 실패: {exc}"
    repo_parts = []
    opts = {"create_github_repo": False}
    index = 0
    while index < len(parts):
        part = parts[index]
        if part == "--github":
            index += 1
            if index >= len(parts) or "/" not in parts[index]:
                return text, None, "--github 값은 owner/repo 형식이어야 합니다."
            owner, repo = parts[index].split("/", 1)
            opts["github_owner"] = owner
            opts["github_repo"] = repo
        elif part == "--project-owner":
            index += 1
            if index >= len(parts) or ":" not in parts[index]:
                return text, None, "--project-owner 값은 org:name 또는 user:name 형식이어야 합니다."
            kind, owner = parts[index].split(":", 1)
            opts["github_project_owner_kind"] = kind
            opts["github_project_owner"] = owner
        elif part == "--create-github-repo":
            opts["create_github_repo"] = True
        else:
            repo_parts.append(part)
        index += 1
    github_keys = {"github_owner", "github_repo", "github_project_owner_kind", "github_project_owner"}
    github_opts = opts if any(k in opts for k in github_keys) else None
    if github_opts and not github_keys.issubset(github_opts):
        return " ".join(repo_parts), None, "GitHub 등록에는 --github owner/repo 와 --project-owner org:name|user:name 이 모두 필요합니다."
    return " ".join(repo_parts), github_opts, None


def merge_project_registration(base: dict, registration: dict) -> dict:
    merged = dict(base)
    for key, value in registration.items():
        if key.startswith("github_") or key in {"repo_path", "name"}:
            merged[key] = value
    return merged
