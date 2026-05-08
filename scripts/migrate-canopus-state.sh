#!/usr/bin/env bash
# scripts/migrate-canopus-state.sh — Canopus multiproject state migration helper.
#
# Plan reference: .omc/plans/canopus-multiproject-state-routing.md §7 PR-B + §8.1.
# Companion: scripts/migrate-canopus-state.ps1 (Windows mirror).
#
# Classifies pre-multiproject Stellaris/.canopus remnants and (with --apply)
# moves them under each task's payload repo_path. Defaults to a safe dry-run
# with --mode=keep. Destructive --apply is blocked when in-flight tasks are
# detected unless --force-with-inflight is also supplied (plan §5.4 D-2 —
# composite predicate matches apps/canopus/src/cli/finalize.rs:116-128 watch
# logic so the scripts and the watch loop share one in-flight definition).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

MODE="keep"
APPLY=0
FORCE_WITH_INFLIGHT=0
STATE_ROOT="${REPO_ROOT}/.canopus"
TASKS_PATH="${TASKS_JSON_PATH:-${REPO_ROOT}/tasks.json}"

usage() {
  cat <<'USAGE'
Usage: migrate-canopus-state.sh [options]

Classify and (optionally) migrate pre-multiproject Stellaris/.canopus
remnants to each task's payload repo_path, per
.omc/plans/canopus-multiproject-state-routing.md §7 PR-B + §8.1.

Options:
  --mode=<move|archive|keep>   Operation mode (default: keep — report only)
                                 move    in-place move artifacts/runs into
                                         <payload_repo>/.canopus
                                 archive rename state_root →
                                         state_root.archived-pre-multiproject
                                 keep    classify+report only, never mutate
  --apply                      Required to perform destructive operations.
                               Without it, all modes are dry-run.
  --force-with-inflight        Override in-flight task guard
                               (plan §5.4 D-2). Default: refuse --apply
                               when in-flight count > 0.
  --tasks-path=<path>          Path to Europa tasks.json (default:
                               $TASKS_JSON_PATH or <repo_root>/tasks.json)
  --state-root=<path>          Pre-multiproject state path to migrate from
                               (default: <repo_root>/.canopus)
  -h, --help                   Show this help and exit.

In-flight predicate (plan §5.4 D-2 — same as finalize.rs:116-128 watch):
  status ∈ {Pending, Dispatched, PendingReview, PendingProposal} OR
  (Processed AND payload.approval_state == "approved"
   AND payload.finalize_requested_at present
   AND <state>/runs/<run_id>-finalize.txt absent)

is_terminal()-only filtering (status.rs:14-16) is INCORRECT here because
Processed+finalize-pending tasks are still in-flight (false-negative).
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode=*) MODE="${1#*=}" ;;
    --mode)
      shift
      if [[ $# -eq 0 ]]; then
        echo "[migrate] --mode requires a value" >&2
        exit 2
      fi
      MODE="$1"
      ;;
    --apply) APPLY=1 ;;
    --force-with-inflight) FORCE_WITH_INFLIGHT=1 ;;
    --tasks-path=*) TASKS_PATH="${1#*=}" ;;
    --tasks-path)
      shift
      if [[ $# -eq 0 ]]; then
        echo "[migrate] --tasks-path requires a value" >&2
        exit 2
      fi
      TASKS_PATH="$1"
      ;;
    --state-root=*) STATE_ROOT="${1#*=}" ;;
    --state-root)
      shift
      if [[ $# -eq 0 ]]; then
        echo "[migrate] --state-root requires a value" >&2
        exit 2
      fi
      STATE_ROOT="$1"
      ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "[migrate] unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

case "$MODE" in
  move|archive|keep) ;;
  *)
    echo "[migrate] invalid --mode: $MODE (expected move|archive|keep)" >&2
    exit 2
    ;;
esac

if ! command -v python3 >/dev/null 2>&1; then
  echo "[migrate] python3 is required" >&2
  exit 3
fi

PYTHONIOENCODING=utf-8 \
MIGRATE_MODE="$MODE" \
MIGRATE_APPLY="$APPLY" \
MIGRATE_FORCE_WITH_INFLIGHT="$FORCE_WITH_INFLIGHT" \
MIGRATE_STATE_ROOT="$STATE_ROOT" \
MIGRATE_TASKS_PATH="$TASKS_PATH" \
exec python3 - <<'PYEOF'
import json
import os
import shutil
import sys
from pathlib import Path

mode = os.environ["MIGRATE_MODE"]
apply_changes = os.environ["MIGRATE_APPLY"] == "1"
force_with_inflight = os.environ["MIGRATE_FORCE_WITH_INFLIGHT"] == "1"
state_root = Path(os.environ["MIGRATE_STATE_ROOT"]).resolve()
tasks_path = Path(os.environ["MIGRATE_TASKS_PATH"])

artifacts_dir = state_root / "artifacts"
runs_dir = state_root / "runs"
orphan_dir = state_root / "orphans"

ACTIVE_STATES = {"Pending", "Dispatched", "PendingReview", "PendingProposal"}


def emit(line):
    print(line, flush=True)


emit(
    f"[migrate] mode={mode} apply={int(apply_changes)} "
    f"force_with_inflight={int(force_with_inflight)}"
)
emit(f"[migrate] state_root={state_root}")
emit(f"[migrate] tasks_path={tasks_path}")

agenda_to_repo = {}
in_flight = []  # (task_id, agenda_id, status)
if tasks_path.exists():
    try:
        tasks = json.loads(tasks_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        emit(f"[migrate] ERROR: tasks.json parse failed: {exc}")
        sys.exit(4)
    if not isinstance(tasks, list):
        emit("[migrate] ERROR: tasks.json must be a JSON array")
        sys.exit(4)
    for task in tasks:
        if not isinstance(task, dict):
            continue
        meta = task.get("meta") or {}
        status = meta.get("status")
        task_id = task.get("task_id")
        payload_raw = task.get("payload")
        payload = None
        if isinstance(payload_raw, dict):
            payload = payload_raw
        elif isinstance(payload_raw, str):
            try:
                parsed = json.loads(payload_raw)
                if isinstance(parsed, dict):
                    payload = parsed
            except json.JSONDecodeError:
                payload = None
        if not payload:
            continue
        agenda_id = (
            payload.get("agenda_id")
            or payload.get("canopus_agenda_id")
            or payload.get("run_id")
            or task_id
        )
        repo_path = payload.get("repo_path")
        if agenda_id and isinstance(repo_path, str) and repo_path.strip():
            agenda_to_repo[agenda_id] = repo_path.strip()

        run_id_for_check = agenda_id or task_id or ""
        finalize_record = (
            (state_root / "runs" / f"{run_id_for_check}-finalize.txt")
            if run_id_for_check
            else None
        )
        is_inflight = False
        if status in ACTIVE_STATES:
            is_inflight = True
        elif status == "Processed":
            approval = payload.get("approval_state")
            finalize_requested = payload.get("finalize_requested_at")
            if (
                approval == "approved"
                and finalize_requested is not None
                and str(finalize_requested).strip()
                and finalize_record is not None
                and not finalize_record.exists()
            ):
                is_inflight = True
        if is_inflight:
            in_flight.append((task_id, agenda_id, status))
else:
    emit(
        f"[migrate] tasks.json not found: {tasks_path} "
        "(continuing with empty mapping)"
    )

emit("")
emit(f"[migrate] in-flight tasks detected: {len(in_flight)}")
for tid, aid, status in in_flight:
    emit(f"[migrate]   - task_id={tid} agenda_id={aid} status={status}")


def derive_agenda(name):
    candidates = sorted(agenda_to_repo.keys(), key=len, reverse=True)
    for key in candidates:
        if name == key or name.startswith(key + "-") or name.startswith(key + "."):
            return key
    return ""


def plan_target(item, agenda_id):
    repo = agenda_to_repo.get(agenda_id)
    if not repo:
        return None
    repo_path = Path(repo)
    if item.parent == artifacts_dir:
        return repo_path / ".canopus" / "artifacts" / item.name
    if item.parent == runs_dir:
        return repo_path / ".canopus" / "runs" / item.name
    return None


artifact_dirs = []
if artifacts_dir.is_dir():
    for entry in sorted(artifacts_dir.iterdir()):
        if entry.is_dir() and entry.name.startswith("agenda-"):
            artifact_dirs.append(entry)

run_jsons = []
finalize_txts = []
if runs_dir.is_dir():
    for entry in sorted(runs_dir.iterdir()):
        if not entry.is_file():
            continue
        if entry.name.endswith("-finalize.txt"):
            finalize_txts.append(entry)
        elif entry.suffix == ".json":
            run_jsons.append(entry)

emit("")
emit(
    f"[migrate] classification: artifacts={len(artifact_dirs)} "
    f"runs_json={len(run_jsons)} finalize_txt={len(finalize_txts)}"
)

classified = []  # list of (item, agenda_id, target | None, base)


def classify(items, label):
    emit("")
    emit(f"[migrate] {label} classification:")
    for item in items:
        if item.name.endswith(".json"):
            base = item.name[:-5]
        elif item.name.endswith("-finalize.txt"):
            base = item.name[: -len("-finalize.txt")]
        else:
            base = item.name
        agenda_id = derive_agenda(base)
        target = plan_target(item, agenda_id) if agenda_id else None
        if target:
            emit(f"[migrate]   {item.name}  ->  {target}")
        else:
            emit(f"[migrate]   {item.name}  ->  ORPHAN (no repo_path mapping)")
        classified.append((item, agenda_id, target, base))


classify(artifact_dirs, "artifact dir")
classify(run_jsons, "runs/*.json")
classify(finalize_txts, "runs/*-finalize.txt")

if not apply_changes:
    emit("")
    emit("[migrate] dry-run: pass --apply to perform destructive operations")
    sys.exit(0)

if mode == "keep":
    emit("")
    emit("[migrate] mode=keep: --apply has no destructive effect")
    sys.exit(0)

if in_flight and not force_with_inflight:
    emit("")
    emit(
        f"[migrate] REFUSING --apply: {len(in_flight)} in-flight task(s) "
        "detected."
    )
    emit(
        "[migrate] re-run with --force-with-inflight to override "
        "(plan §5.4 D-2 guard)."
    )
    sys.exit(5)

if mode == "archive":
    if not state_root.exists():
        emit("[migrate] nothing to archive: state_root missing")
        sys.exit(0)
    target = state_root.with_name(state_root.name + ".archived-pre-multiproject")
    if target.exists():
        emit(
            f"[migrate] archive target already exists; treating as idempotent: {target}"
        )
        sys.exit(0)
    state_root.rename(target)
    emit(f"[migrate] archived {state_root} -> {target}")
    sys.exit(0)

# mode == "move"
moved = 0
skipped = 0
orphaned = 0
orphan_dir.mkdir(parents=True, exist_ok=True)
for item, agenda_id, target, base in classified:
    if not item.exists():
        skipped += 1
        continue
    if not target:
        orphan_key = agenda_id or base or item.name
        orphan_target = orphan_dir / orphan_key / item.name
        orphan_target.parent.mkdir(parents=True, exist_ok=True)
        if orphan_target.exists():
            emit(f"[migrate] skip orphan (already moved): {item.name}")
            skipped += 1
            continue
        shutil.move(str(item), str(orphan_target))
        emit(f"[migrate] orphan moved: {item.name} -> {orphan_target}")
        orphaned += 1
        continue
    if target.exists():
        emit(f"[migrate] skip (target exists, idempotent): {item.name}")
        skipped += 1
        continue
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(str(item), str(target))
    emit(f"[migrate] moved: {item.name} -> {target}")
    moved += 1

emit("")
emit(f"[migrate] summary: moved={moved} orphaned={orphaned} skipped={skipped}")
PYEOF
