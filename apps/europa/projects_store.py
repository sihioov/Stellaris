"""Project registration persistence helpers for the Stellaris Discord bot."""
import json
import os
import re
import shlex
import tempfile

from config import PROJECTS_JSON_PATH, TASKS_DIR, TASKS_JSON_PATH


WORKTREE_DEFAULT_NAME = "default"
_WORKTREE_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")


def validate_worktree_name(name: str | None) -> str | None:
    """Return an error string when a Discord worktree name is unsafe."""
    if not name or not name.strip():
        return "worktree 이름이 필요합니다."
    value = name.strip()
    if value.startswith(".") or ".." in value:
        return "worktree 이름에는 선행 점이나 `..` 을 사용할 수 없습니다."
    if not _WORKTREE_NAME_RE.fullmatch(value):
        return "worktree 이름은 영문/숫자로 시작하고 영문, 숫자, `.`, `_`, `-` 만 사용할 수 있습니다."
    return None


def _worktree_entry(repo_path: str, created_at: str | None = None) -> dict:
    entry = {"repo_path": repo_path}
    if created_at:
        entry["created_at"] = created_at
    return entry


def normalize_project_worktrees(project: dict) -> dict:
    """Return a backward-compatible project record with worktree fields hydrated."""
    normalized = dict(project)
    repo_path = str(normalized.get("repo_path") or "")
    base_repo_path = str(normalized.get("base_repo_path") or repo_path)
    normalized["base_repo_path"] = base_repo_path

    raw_worktrees = normalized.get("worktrees")
    worktrees = {}
    if isinstance(raw_worktrees, dict):
        for name, entry in raw_worktrees.items():
            if isinstance(entry, dict):
                path = entry.get("repo_path")
                if path:
                    worktrees[str(name)] = dict(entry)
            elif isinstance(entry, str) and entry:
                worktrees[str(name)] = _worktree_entry(entry)

    if WORKTREE_DEFAULT_NAME not in worktrees and base_repo_path:
        worktrees[WORKTREE_DEFAULT_NAME] = _worktree_entry(base_repo_path)

    active = str(normalized.get("active_worktree") or WORKTREE_DEFAULT_NAME)
    if active not in worktrees and repo_path:
        worktrees[active] = _worktree_entry(repo_path)

    normalized["active_worktree"] = active
    normalized["worktrees"] = worktrees
    if not normalized.get("repo_path") and active in worktrees:
        normalized["repo_path"] = worktrees[active]["repo_path"]
    return normalized


def record_project_worktree(project: dict, name: str, repo_path: str, created_at: str) -> dict:
    normalized = normalize_project_worktrees(project)
    normalized["worktrees"][name] = _worktree_entry(repo_path, created_at)
    return normalized


def switch_project_worktree(project: dict, name: str) -> tuple[dict | None, str | None]:
    normalized = normalize_project_worktrees(project)
    entry = normalized["worktrees"].get(name)
    if not entry:
        return None, f"알 수 없는 worktree입니다: `{name}`"
    repo_path = entry.get("repo_path")
    if not repo_path:
        return None, f"worktree 경로가 비어 있습니다: `{name}`"
    normalized["repo_path"] = repo_path
    normalized["active_worktree"] = name
    return normalized, None


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
