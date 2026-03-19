#!/usr/bin/env bash
# Multi-n experiment: test workers on different vertex counts simultaneously
# Tests: multi-n support, history snapshots, multi-user, export
#
# Usage: ./scripts/experiment-multi-n.sh [--duration MINS]
set -euo pipefail
cd "$(dirname "$0")/.."

DURATION_MINS=30
while [[ $# -gt 0 ]]; do
    case $1 in
        --duration) DURATION_MINS=$2; shift 2 ;;
        *) echo "Unknown: $1"; exit 1 ;;
    esac
done

DURATION_SECS=$((DURATION_MINS * 60))
COMMIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
LOG_DIR="logs/experiment-multi-n-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$LOG_DIR"

echo "══════════════════════════════════════════════"
echo "  MineGraph Multi-n Experiment"
echo "══════════════════════════════════════════════"
echo ""
echo "  Duration:   ${DURATION_MINS} minutes"
echo "  Commit:     $COMMIT_HASH"
echo "  Workers:    6 total (2 per n value)"
echo "  Targets:    n=17 R(4,4), n=25 R(5,5), n=9 R(3,4)"
echo "  Capacity:   2000"
echo ""

# ── Prerequisites ────────────────────────────────
echo "Step 1: Ensure server is running with fresh database"
echo ""
echo "  In another terminal:"
echo "    cargo run --release -p minegraph-server -- \\"
echo "      --migrate --server-key .config/minegraph/server-key.json \\"
echo "      --leaderboard-capacity 2000"
echo ""
echo "  Then register your key:"
echo "    cargo run -p minegraph-cli -- register-key"
echo ""
read -p "Press Enter when server is ready..."

# Verify server
if ! curl -s http://localhost:3001/api/health > /dev/null 2>&1; then
    echo "ERROR: Server not reachable"
    exit 1
fi

echo ""
echo "Step 2: Building worker..."
cargo build --release -p minegraph-worker 2>&1 | tail -1
WORKER_BIN="target/release/minegraph-worker"

# ── Save config ──────────────────────────────────
cat > "$LOG_DIR/config.json" <<EOF
{
    "experiment": "multi-n",
    "duration_mins": $DURATION_MINS,
    "commit_hash": "$COMMIT_HASH",
    "targets": [
        {"n": 9, "k": 3, "ell": 4, "workers": 2, "note": "R(3,4)=9, should saturate fast"},
        {"n": 17, "k": 4, "ell": 4, "workers": 2, "note": "R(4,4)=18, n=17 is solvable"},
        {"n": 25, "k": 5, "ell": 5, "workers": 2, "note": "R(5,5)>=43, n=25 is deep"}
    ],
    "started": "$(date -Iseconds)"
}
EOF

echo ""
echo "Step 3: Launching workers..."
echo ""

PIDS=()
STOPPED=0

cleanup() {
    if [[ "$STOPPED" -eq 1 ]]; then return; fi
    STOPPED=1
    echo ""
    echo "Stopping workers..."
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true

    echo ""
    echo "══════════════════════════════════════════════"
    echo "  RESULTS"
    echo "══════════════════════════════════════════════"
    echo ""

    # Leaderboard status per n
    for N in 9 17 25; do
        THRESHOLD=$(curl -s "http://localhost:3001/api/leaderboards/$N/threshold" 2>/dev/null)
        COUNT=$(echo "$THRESHOLD" | python3 -c "import sys,json; print(json.load(sys.stdin).get('count','?'))" 2>/dev/null || echo "?")
        echo "  n=$N: $COUNT entries"
    done
    echo ""

    # Worker summaries
    echo "  Worker details:"
    for LOG in "$LOG_DIR"/worker-*.log; do
        NAME=$(basename "$LOG" .log)
        CLEAN=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" 2>/dev/null)
        ROUNDS=$(echo "$CLEAN" | grep -c "round complete" 2>/dev/null || echo 0)
        DISC=$(echo "$CLEAN" | grep -o 'total_discoveries=[0-9]*' | tail -1 | grep -o '[0-9]*' || echo 0)
        ADMIT=$(echo "$CLEAN" | grep -o 'total_admitted=[0-9]*' | tail -1 | grep -o '[0-9]*' || echo 0)
        echo "    $NAME: $ROUNDS rounds, $DISC discovered, $ADMIT admitted"
    done

    echo ""
    echo "  Export leaderboards:"
    for N in 9 17 25; do
        FILE="$LOG_DIR/leaderboard-n$N.g6"
        curl -s "http://localhost:3001/api/leaderboards/$N/export" > "$FILE" 2>/dev/null
        LINES=$(wc -l < "$FILE" 2>/dev/null || echo 0)
        echo "    n=$N: $LINES graphs -> $FILE"
    done

    echo ""
    echo "  Logs: $LOG_DIR"
    echo ""
}
trap cleanup INT TERM EXIT

# ── Launch workers ───────────────────────────────

# n=9, R(3,4) — small, should find lots of valid graphs quickly
for i in 1 2; do
    NO_COLOR=1 RUST_LOG=info "$WORKER_BIN" \
        --server http://localhost:3001 \
        --n 9 --target-k 3 --target-ell 4 \
        --beam-width 50 --max-depth 8 \
        --sample-bias 0.8 --max-iters 50000 \
        --metadata "{\"worker_id\":\"r34-$i\",\"commit_hash\":\"$COMMIT_HASH\",\"target\":\"R(3,4)\",\"n\":9}" \
        > "$LOG_DIR/worker-r34-$i.log" 2>&1 &
    PIDS+=($!)
    echo "  Started r34-$i (n=9, R(3,4))"
done

# n=17, R(4,4) — medium, Paley(17) is a known good seed
for i in 1 2; do
    NO_COLOR=1 RUST_LOG=info "$WORKER_BIN" \
        --server http://localhost:3001 \
        --n 17 --target-k 4 --target-ell 4 \
        --beam-width 60 --max-depth 10 \
        --sample-bias 0.8 --max-iters 80000 \
        --metadata "{\"worker_id\":\"r44-$i\",\"commit_hash\":\"$COMMIT_HASH\",\"target\":\"R(4,4)\",\"n\":17}" \
        > "$LOG_DIR/worker-r44-$i.log" 2>&1 &
    PIDS+=($!)
    echo "  Started r44-$i (n=17, R(4,4))"
done

# n=25, R(5,5) — the main target, deep search
for i in 1 2; do
    NOISE=$((i * 2))
    NO_COLOR=1 RUST_LOG=info "$WORKER_BIN" \
        --server http://localhost:3001 \
        --n 25 --target-k 5 --target-ell 5 \
        --beam-width 80 --max-depth 12 \
        --sample-bias 0.8 --max-iters 100000 \
        --noise-flips "$NOISE" \
        --metadata "{\"worker_id\":\"r55-$i\",\"commit_hash\":\"$COMMIT_HASH\",\"target\":\"R(5,5)\",\"n\":25,\"noise\":$NOISE}" \
        > "$LOG_DIR/worker-r55-$i.log" 2>&1 &
    PIDS+=($!)
    echo "  Started r55-$i (n=25, R(5,5), noise=$NOISE)"
done

printf '%s\n' "${PIDS[@]}" > "$LOG_DIR/pids"

echo ""
echo "Running for ${DURATION_MINS}m (6 workers across 3 targets)."
echo ""
echo "Monitor:"
echo "  tail -f $LOG_DIR/worker-r55-1.log"
echo "  http://localhost:5173/leaderboards     (browse all n)"
echo "  http://localhost:5173/dashboard         (worker stats)"
echo ""

# ── Status loop ──────────────────────────────────
ELAPSED=0
INTERVAL=60
while [[ $ELAPSED -lt $DURATION_SECS ]]; do
    sleep "$INTERVAL"
    ELAPSED=$((ELAPSED + INTERVAL))
    MINS=$((ELAPSED / 60))

    ALIVE=0
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then ALIVE=$((ALIVE + 1)); fi
    done

    # Collect counts per n
    STATUS=""
    for N in 9 17 25; do
        COUNT=$(curl -s "http://localhost:3001/api/leaderboards/$N/threshold" 2>/dev/null \
            | python3 -c "import sys,json; print(json.load(sys.stdin).get('count',0))" 2>/dev/null || echo "?")
        STATUS="$STATUS n$N=$COUNT"
    done

    echo "[${MINS}m] Workers: $ALIVE/6 |$STATUS"

    if [[ "$ALIVE" -eq 0 ]]; then
        echo "All workers exited."
        break
    fi
done
