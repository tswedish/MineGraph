#!/usr/bin/env bash
# Keep the orchestrator (or any command) alive, restarting on crash.
#
# Usage:
#   ./scripts/keep-alive.sh                               # default: agent-orchestrate
#   ./scripts/keep-alive.sh ./run agent-loop               # custom command
#   ./scripts/keep-alive.sh ./run agent-orchestrate --local # with args
#
# Restarts after 30s cooldown on failure. Ctrl-C to stop permanently.

set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $# -gt 0 ]]; then
    CMD=("$@")
else
    CMD=(./run agent-orchestrate)
fi
COOLDOWN=30
KEEP_ALIVE_LOG="logs/keep-alive.log"
mkdir -p logs

log() {
    local msg="$(date '+%Y-%m-%d %H:%M:%S') $1"
    echo "$msg"
    echo "$msg" >> "$KEEP_ALIVE_LOG"
}

log "Keep-alive: will restart '${CMD[*]}' on failure"
echo "  Ctrl-C to stop permanently"
echo ""

RESTART_COUNT=0
while true; do
    log "Starting (restart #$RESTART_COUNT): ${CMD[*]}"
    "${CMD[@]}" && break  # clean exit = stop
    EXIT_CODE=$?
    echo ""
    log "Process exited with code $EXIT_CODE"
    RESTART_COUNT=$((RESTART_COUNT + 1))
    log "Restarting in ${COOLDOWN}s... (Ctrl-C to stop)"
    sleep "$COOLDOWN"
done
