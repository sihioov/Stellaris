#!/usr/bin/env bash
set -euo pipefail

SESSION_NAME="${STELLARIS_TMUX_SESSION:-stellaris}"
START_DELAY="${STELLARIS_TMUX_START_DELAY:-2}"
ATTACH=1
DRY_RUN=0

usage() {
  cat <<'USAGE'
Usage: ./start-pipeline.sh [--session NAME] [--delay SECONDS] [--no-attach] [--dry-run]

Starts the local Stellaris runtime in one tmux window split into panes:
  1. ton618/run.sh
  2. laniakea/run.sh
  3. apps/europa/run.sh

Environment overrides:
  STELLARIS_TMUX_SESSION      tmux session name (default: stellaris)
  STELLARIS_TMUX_START_DELAY  seconds between service starts (default: 2)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      [[ $# -ge 2 ]] || { echo "error: --session requires a name" >&2; exit 2; }
      SESSION_NAME="$2"
      shift 2
      ;;
    --delay)
      [[ $# -ge 2 ]] || { echo "error: --delay requires seconds" >&2; exit 2; }
      START_DELAY="$2"
      shift 2
      ;;
    --no-attach)
      ATTACH=0
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! "$START_DELAY" =~ ^[0-9]+$ ]]; then
  echo "error: --delay must be a non-negative integer" >&2
  exit 2
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"
TASKS_JSON="$REPO_ROOT/tasks.json"
CANOPUS_STATE="$REPO_ROOT/.canopus"

SERVICES=(
  "ton618|TON618 scheduler|./ton618/run.sh"
  "laniakea|Laniakea worker|./laniakea/run.sh"
  "europa|Europa Discord bot|./apps/europa/run.sh"
)

require_executable() {
  local path="$1"
  if [[ ! -x "$REPO_ROOT/$path" ]]; then
    echo "error: $path is missing or not executable" >&2
    exit 1
  fi
}

for spec in "${SERVICES[@]}"; do
  IFS='|' read -r _name _label command <<< "$spec"
  require_executable "${command#./}"
done

if (( DRY_RUN )); then
  echo "Dry-run: would create tmux session '$SESSION_NAME' from $REPO_ROOT"
  echo "Shared defaults: TASKS_JSON_PATH=$TASKS_JSON, LANIAKEA_FILE_PATH=$TASKS_JSON, CANOPUS_STATE_PATH=$CANOPUS_STATE"
  echo "Layout: ton618 on the left, laniakea on the upper right, europa on the lower right"
  for spec in "${SERVICES[@]}"; do
    IFS='|' read -r name label command <<< "$spec"
    echo "- pane '$name' ($label): $command"
  done
  exit 0
fi

if ! command -v tmux >/dev/null 2>&1; then
  echo "error: tmux is not installed or not on PATH" >&2
  exit 1
fi

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "tmux session '$SESSION_NAME' already exists. Reusing it instead of starting duplicates."
  if (( ATTACH )); then
    if [[ -n "${TMUX:-}" ]]; then
      tmux switch-client -t "$SESSION_NAME"
    else
      tmux attach-session -t "$SESSION_NAME"
    fi
  else
    echo "Attach with: tmux attach -t $SESSION_NAME"
  fi
  exit 0
fi

make_service_command() {
  local label="$1"
  local command="$2"
  local quoted_root quoted_tasks quoted_state
  printf -v quoted_root '%q' "$REPO_ROOT"
  printf -v quoted_tasks '%q' "$TASKS_JSON"
  printf -v quoted_state '%q' "$CANOPUS_STATE"

  cat <<EOF_CMD
cd $quoted_root
export TASKS_JSON_PATH="\${TASKS_JSON_PATH:-$quoted_tasks}"
export LANIAKEA_FILE_PATH="\${LANIAKEA_FILE_PATH:-$quoted_tasks}"
export LANIAKEA_SOURCE="\${LANIAKEA_SOURCE:-file}"
export CANOPUS_REPO_PATH="\${CANOPUS_REPO_PATH:-$quoted_root}"
export CANOPUS_STATE_PATH="\${CANOPUS_STATE_PATH:-$quoted_state}"
export RUST_LOG="\${RUST_LOG:-info}"
echo "[$label] starting: $command"
echo "[$label] repo: $REPO_ROOT"
$command
status=\$?
echo
echo "[$label] exited with status \$status"
exec "\${SHELL:-/bin/bash}"
EOF_CMD
}

start_service_pane() {
  local index="$1"
  local pane_name="$2"
  local label="$3"
  local command="$4"
  local shell_command
  shell_command="$(make_service_command "$label" "$command")"

  case "$index" in
    0)
      tmux new-session -d -P -F "#{pane_id}" -s "$SESSION_NAME" -n "pipeline" -c "$REPO_ROOT" \
        bash -lc "$shell_command"
      ;;
    1)
      tmux split-window -h -l 50% -P -F "#{pane_id}" -t "$TON618_PANE" -c "$REPO_ROOT" \
        bash -lc "$shell_command"
      ;;
    2)
      tmux split-window -v -l 50% -P -F "#{pane_id}" -t "$LANIAKEA_PANE" -c "$REPO_ROOT" \
        bash -lc "$shell_command"
      ;;
    *)
      echo "error: unsupported service index: $index" >&2
      exit 1
      ;;
  esac
}

echo "Starting tmux session '$SESSION_NAME'..."
TON618_PANE=""
LANIAKEA_PANE=""
EUROPA_PANE=""
for index in "${!SERVICES[@]}"; do
  IFS='|' read -r name label command <<< "${SERVICES[$index]}"
  echo "[$((index + 1))/${#SERVICES[@]}] $label -> $command"
  pane_id="$(start_service_pane "$index" "$name" "$label" "$command")"
  tmux select-pane -t "$pane_id" -T "$name"
  case "$name" in
    ton618) TON618_PANE="$pane_id" ;;
    laniakea) LANIAKEA_PANE="$pane_id" ;;
    europa) EUROPA_PANE="$pane_id" ;;
  esac
  if (( index + 1 < ${#SERVICES[@]} && START_DELAY > 0 )); then
    sleep "$START_DELAY"
  fi
done

tmux set-option -t "$SESSION_NAME" pane-border-status top >/dev/null
tmux set-option -t "$SESSION_NAME" pane-border-format " #{pane_title} " >/dev/null
tmux select-pane -t "$TON618_PANE"
echo "Started ${#SERVICES[@]} service panes in tmux session '$SESSION_NAME'."

if (( ATTACH )); then
  if [[ -n "${TMUX:-}" ]]; then
    tmux switch-client -t "$SESSION_NAME"
  else
    tmux attach-session -t "$SESSION_NAME"
  fi
else
  echo "Attach with: tmux attach -t $SESSION_NAME"
fi
