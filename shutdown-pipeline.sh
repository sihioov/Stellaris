#!/usr/bin/env bash
set -euo pipefail

SESSION_NAME="${STELLARIS_TMUX_SESSION:-stellaris}"
GRACE_SECONDS="${STELLARIS_TMUX_SHUTDOWN_GRACE:-5}"
DRY_RUN=0

usage() {
  cat <<'USAGE'
Usage: ./shutdown-pipeline.sh [--session NAME] [--grace SECONDS] [--dry-run]

Stops the local Stellaris tmux runtime started by ./start-pipeline.sh:
  1. sends Ctrl-C to every pane in the tmux session
  2. waits briefly for services to exit
  3. kills the tmux session

Environment overrides:
  STELLARIS_TMUX_SESSION         tmux session name (default: stellaris)
  STELLARIS_TMUX_SHUTDOWN_GRACE  seconds to wait after Ctrl-C (default: 5)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      [[ $# -ge 2 ]] || { echo "error: --session requires a name" >&2; exit 2; }
      SESSION_NAME="$2"
      shift 2
      ;;
    --grace)
      [[ $# -ge 2 ]] || { echo "error: --grace requires seconds" >&2; exit 2; }
      GRACE_SECONDS="$2"
      shift 2
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

if [[ ! "$GRACE_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "error: --grace must be a non-negative integer" >&2
  exit 2
fi

if ! command -v tmux >/dev/null 2>&1; then
  echo "error: tmux is not installed or not on PATH" >&2
  exit 1
fi

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "tmux session '$SESSION_NAME' is not running. Nothing to shut down."
  exit 0
fi

mapfile -t PANES < <(tmux list-panes -t "$SESSION_NAME" -F '#{pane_id}|#{pane_title}')

if (( DRY_RUN )); then
  echo "Dry-run: would shut down tmux session '$SESSION_NAME'"
  for pane in "${PANES[@]}"; do
    IFS='|' read -r pane_id pane_title <<< "$pane"
    echo "- send Ctrl-C to ${pane_title:-unnamed} ($pane_id)"
  done
  echo "- wait ${GRACE_SECONDS}s"
  echo "- kill tmux session '$SESSION_NAME'"
  exit 0
fi

echo "Stopping tmux session '$SESSION_NAME'..."
for pane in "${PANES[@]}"; do
  IFS='|' read -r pane_id pane_title <<< "$pane"
  echo "- stopping ${pane_title:-unnamed} ($pane_id)"
  tmux send-keys -t "$pane_id" C-c
done

if (( GRACE_SECONDS > 0 )); then
  sleep "$GRACE_SECONDS"
fi

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  tmux kill-session -t "$SESSION_NAME"
fi
echo "Stopped tmux session '$SESSION_NAME'."
