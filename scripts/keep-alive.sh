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

CMD="${@:-./run agent-orchestrate}"
COOLDOWN=30

echo "Keep-alive: will restart '$CMD' on failure"
echo "  Ctrl-C to stop permanently"
echo ""

while true; do
    echo "$(date '+%Y-%m-%d %H:%M:%S') Starting: $CMD"
    $CMD && break  # clean exit = stop
    EXIT_CODE=$?
    echo ""
    echo "$(date '+%Y-%m-%d %H:%M:%S') Process exited with code $EXIT_CODE"
    echo "  Restarting in ${COOLDOWN}s... (Ctrl-C to stop)"
    sleep "$COOLDOWN"
done
