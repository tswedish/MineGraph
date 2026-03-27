#!/usr/bin/env bash
# ── Extremal Meta-Agent Orchestrator ────────────────────────────────
#
# Decides whether to run strategy research or experiments, then
# dispatches the appropriate agent. Runs in a loop, alternating
# between research and experimentation based on current state.
#
# Usage:
#   ./scripts/orchestrator.sh                    # Production, auto-decide
#   ./scripts/orchestrator.sh --local            # Local dev server
#   ./scripts/orchestrator.sh --research         # Force research mode
#   ./scripts/orchestrator.sh --experiment       # Force experiment mode
#   ./scripts/orchestrator.sh --workers 8        # Pass through to experiment
#
# Architecture:
#   orchestrator.sh
#     ├── decides: research or experiment?
#     ├── research → Claude + strategy-research.md skill → code changes + git commit
#     └── experiment → loop.sh → fleet + Claude observe-decide-act cycles
#
# The orchestrator reads experiments/agent/strategies.json to find
# untested strategies, and experiments/agent/journal.md to gauge
# whether experiments are plateauing (time to research) or active
# (keep experimenting).

set -euo pipefail
cd "$(dirname "$0")/.."

# ── Defaults ─────────────────────────────────────────────
SERVER="https://api.extremal.online"
MODE="auto"          # auto, research, experiment
WORKERS=4
POLISH=100
EXPERIMENT_INTERVAL="5m"
EXPERIMENT_CYCLES=5  # how many experiment cycles before re-evaluating
RESEARCH_BUDGET="5.00"
EXPERIMENT_BUDGET="1.00"
MODEL="opus"

# ── Parse args ───────────────────────────────────────────
PASSTHROUGH_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --local)       SERVER="http://localhost:3001"; PASSTHROUGH_ARGS+=("$1"); shift ;;
        --research)    MODE="research"; shift ;;
        --experiment)  MODE="experiment"; shift ;;
        --workers)     WORKERS="$2"; PASSTHROUGH_ARGS+=("$1" "$2"); shift 2 ;;
        --polish)      POLISH="$2"; PASSTHROUGH_ARGS+=("$1" "$2"); shift 2 ;;
        --interval)    EXPERIMENT_INTERVAL="$2"; PASSTHROUGH_ARGS+=("$1" "$2"); shift 2 ;;
        --cycles)      EXPERIMENT_CYCLES="$2"; shift 2 ;;
        --model)       MODEL="$2"; shift 2 ;;
        *)             PASSTHROUGH_ARGS+=("$1"); shift ;;
    esac
done

BRANCH=$(git branch --show-current 2>/dev/null || echo "unknown")
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
LOG_DIR="logs/orchestrator-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$LOG_DIR"

echo "╔══════════════════════════════════════════════════╗"
echo "║  Extremal Meta-Agent Orchestrator                ║"
echo "╠══════════════════════════════════════════════════╣"
echo "║  server:   $SERVER"
echo "║  branch:   $BRANCH ($COMMIT)"
echo "║  mode:     $MODE"
echo "║  model:    $MODEL"
echo "║  logs:     $LOG_DIR"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# ── Cleanup ──────────────────────────────────────────────
STOPPED=0
cleanup() {
    if [[ "$STOPPED" -eq 1 ]]; then return; fi
    STOPPED=1
    echo ""
    echo "Orchestrator shutting down..."
    # Kill any experiment fleet
    pkill -f "extremal-worker" 2>/dev/null || true
    sleep 1
    echo "Done. Logs: $LOG_DIR"
}
trap cleanup INT TERM EXIT

# ── Decision function ────────────────────────────────────
decide_mode() {
    # Returns "research" or "experiment" based on current state
    python3 -c "
import json, sys, os

# Load strategy registry
try:
    reg = json.load(open('experiments/agent/strategies.json'))
except:
    print('research')  # No registry = need to set up
    sys.exit(0)

strategies = reg.get('strategies', [])
ideas = reg.get('ideas', [])

# Count untested strategies
untested = [s for s in strategies if s.get('status') == 'untested']

# Count high-priority ideas
high_ideas = [i for i in ideas if i.get('priority') == 'high']

# Check if experiments are plateauing (read journal)
plateau = False
try:
    journal = open('experiments/agent/journal.md').read()
    # Simple heuristic: if last 3 entries are all ADJUST with marginal improvements
    lines = [l for l in journal.split('\n') if 'marginal' in l.lower() or 'plateau' in l.lower()]
    if len(lines) >= 2:
        plateau = True
except:
    pass

# Decision logic:
# 1. If untested strategies exist → experiment (test them)
# 2. If experiments are plateauing and there are high-priority ideas → research
# 3. If no ideas left → experiment (keep running best config)
# 4. Default: experiment (collecting data is usually more valuable)

if untested:
    print('experiment')
elif plateau and high_ideas:
    print('research')
elif not high_ideas and not untested:
    print('research')  # Need new ideas
else:
    print('experiment')
" 2>/dev/null || echo "experiment"
}

# ── Main loop ────────────────────────────────────────────
ROUND=0
while true; do
    ROUND=$((ROUND + 1))
    COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
    NOW=$(date '+%Y-%m-%d %H:%M')

    echo ""
    echo "╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍"
    echo "  Orchestrator Round #$ROUND  [$NOW]  commit=$COMMIT"
    echo "╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍╍"

    # Decide mode
    if [[ "$MODE" == "auto" ]]; then
        CHOSEN=$(decide_mode)
    else
        CHOSEN="$MODE"
    fi

    echo "  Decision: $CHOSEN"
    echo ""

    if [[ "$CHOSEN" == "research" ]]; then
        # ── Research Phase ───────────────────────────────
        echo "  ▶ Running strategy research..."

        RESEARCH_PROMPT=$(cat <<'RESEARCH_EOF'
Run one strategy research cycle per the strategy-research skill protocol.

Read the current state:
1. experiments/agent/strategies.json — strategy registry with ideas and validated strategies
2. experiments/agent/findings.json — experimental findings
3. experiments/agent/journal.md — recent experiment activity

Then: assess → choose ONE idea → implement → run CI → commit → update registry → report.

Stay on the current git branch. One change only. CI must pass.
RESEARCH_EOF
)

        echo "$RESEARCH_PROMPT" | claude \
            --print \
            --model "$MODEL" \
            --effort max \
            --append-system-prompt-file "skills/strategy-research.md" \
            --allowed-tools "Bash(*) Read(*) Edit(*) Write(*) Grep(*) Glob(*)" \
            --max-budget-usd "$RESEARCH_BUDGET" \
            --no-session-persistence \
            2>&1 | tee "$LOG_DIR/research-$ROUND.log"

        echo ""
        echo "  Research cycle complete. New commit: $(git rev-parse --short HEAD)"
        echo "  Rebuilding worker binary..."
        cargo build --release -p extremal-worker 2>&1 | tail -1

    else
        # ── Experiment Phase ─────────────────────────────
        echo "  ▶ Running experiment loop ($EXPERIMENT_CYCLES cycles)..."

        # Build the experiment prompt with cycle limit
        # We run loop.sh but with a cycle limit by using timeout or a wrapper
        # Instead, let's run the loop inline for N cycles

        # First ensure fleet is running
        if ! pgrep -f "extremal-worker" > /dev/null 2>&1; then
            echo "  Launching fleet first..."
            # Launch fleet in background (without the loop part)
            ./scripts/agent-fleet.sh --workers "$WORKERS" --n 25 --polish "$POLISH" \
                --server "$SERVER" &
            FLEET_PID=$!
            sleep 35  # Wait for fleet warmup
        fi

        # Find the most recent log dir
        FLEET_LOG=$(ls -td logs/agent-* 2>/dev/null | head -1)

        for CYCLE in $(seq 1 "$EXPERIMENT_CYCLES"); do
            echo ""
            echo "  ── Experiment Cycle $CYCLE/$EXPERIMENT_CYCLES ──"

            STATUS_FILE=$(mktemp)
            ./scripts/agent-status.sh "$FLEET_LOG" > "$STATUS_FILE" 2>&1 || true

            EXPERIMENT_PROMPT=$(cat <<EXPERIMENT_EOF
Run one experiment agent observe-decide-act cycle per the experiment skill protocol.

## Current Fleet Status
$(cat "$STATUS_FILE")

## Session Context
- Orchestrator round: #$ROUND, experiment cycle: $CYCLE/$EXPERIMENT_CYCLES
- Server: $SERVER
- Log dir: $FLEET_LOG

Analyze the status. Adjust workers via direct HTTP API if needed.
Find worker ports: curl -sf http://localhost:4000/api/workers
Config changes: curl -sf -X POST http://localhost:PORT/api/config -H "Content-Type: application/json" -d '{"param": value}'

Log actions to experiments/agent/journal.md.

Output a concise cycle report:
  ## Cycle $CYCLE/$EXPERIMENT_CYCLES [$(date '+%H:%M')]
  **Leaderboard**: [entry count, top score, our count]
  **Fleet**: [total rounds, total admits, best worker rate]
  **Action**: [what you did, or "none"]
  **Next**: [what to watch for]
EXPERIMENT_EOF
)

            echo "$EXPERIMENT_PROMPT" | claude \
                --print \
                --model "$MODEL" \
                --effort max \
                --append-system-prompt-file "skills/experiment.md" \
                --allowed-tools "Bash(*) Read(*) Edit(*) Write(*) Grep(*) Glob(*)" \
                --max-budget-usd "$EXPERIMENT_BUDGET" \
                --no-session-persistence \
                2>&1 | tee "$LOG_DIR/experiment-${ROUND}-${CYCLE}.log"

            rm -f "$STATUS_FILE"

            if [[ $CYCLE -lt $EXPERIMENT_CYCLES ]]; then
                echo "  Next experiment cycle in $EXPERIMENT_INTERVAL..."
                INTERVAL_SECS=$(python3 -c "
s='$EXPERIMENT_INTERVAL'
n=int(s[:-1]); u=s[-1]
print(n*3600 if u=='h' else n*60 if u=='m' else n)
" 2>/dev/null || echo 300)
                sleep "$INTERVAL_SECS"
            fi
        done

        echo ""
        echo "  Experiment phase complete ($EXPERIMENT_CYCLES cycles)."
        echo "  Stopping fleet for potential research..."
        pkill -f "extremal-worker" 2>/dev/null || true
        sleep 2
    fi

    echo ""
    echo "  Round #$ROUND complete. Deciding next action..."
    sleep 5
done
