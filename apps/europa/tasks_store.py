"""Task queue JSON persistence helpers for the Stellaris Discord bot."""
import json
import os
import tempfile
from contextlib import contextmanager

from payloads import now_iso

try:
    import fcntl
except ImportError:  # pragma: no cover - Windows/dev fallback
    fcntl = None


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
    mutate=None,
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
        if mutate is not None:
            mutate(target)
        snapshot = json.loads(json.dumps(target))
        _write_tasks_unlocked(tasks, path)
        return snapshot, None


def find_single_task_locked(
    path: str,
    task_id: str | None,
    command: str,
    statuses: set[str],
    label: str,
) -> tuple[dict | None, str | None]:
    with task_file_lock(path, exclusive=False):
        tasks = _read_tasks_unlocked(path)
        candidates = [t for t in tasks if t.get("meta", {}).get("status") in statuses]
        if task_id:
            target = next((t for t in tasks if t.get("task_id") == task_id), None)
            if not target:
                return None, f"⚠️ Task `{task_id}` 를 찾을 수 없습니다."
            current = target.get("meta", {}).get("status", "?")
            if current not in statuses:
                return None, f"⚠️ Task `{task_id}` 는 {label} 상태가 아닙니다 (현재: {current})."
            return json.loads(json.dumps(target)), None
        if not candidates:
            return None, f"⚠️ {label} 상태의 태스크가 없습니다."
        if len(candidates) > 1:
            ids = ", ".join(f"`{t['task_id']}`" for t in candidates)
            return None, f"⚠️ 여러 태스크가 대상입니다: {ids}\n`!{command} <task_id>` 로 지정해주세요."
        return json.loads(json.dumps(candidates[0])), None
