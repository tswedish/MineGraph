#!/usr/bin/env bash
# Launch a fleet of workers against a server.
#
# Usage:
#   ./scripts/fleet.sh [OPTIONS]
#
# Options:
#   --workers N       Number of workers (default: 16)
#   --strategy STR    Strategy for all workers (default: tree2)
#   --k K             Ramsey parameter k (default: 5)
#   --ell L           Ramsey parameter ell (default: 5)
#   --n N             Target vertex count (default: 25)
#   --server URL      Server URL (default: http://localhost:3001)
#   --init MODE       Init mode (default: leaderboard)
#   --base-port PORT  First dashboard port (default: 8080)
#   --max-iters N     Max iterations per round (default: 100000)
#
# This script:
#   1. Builds release binaries
#   2. Launches N workers with sequential dashboard ports
#   3. Logs each worker to logs/fleet-<timestamp>/<strategy>-<N>.log
#   4. Prints all dashboard URLs for easy tab-opening
#   5. On Ctrl+C, stops all workers and prints a summary

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

source "$HOME/.cargo/env" 2>/dev/null || true

# Defaults
NUM_WORKERS=16
STRATEGY="tree2"
K=5
ELL=5
N=25
SERVER_URL="http://localhost:3001"
INIT_MODE="leaderboard"
BASE_PORT=8080
MAX_ITERS=100000

# Parse args
while [[ $# -gt 0 ]]; do
  case $1 in
    --workers) NUM_WORKERS="$2"; shift 2 ;;
    --strategy) STRATEGY="$2"; shift 2 ;;
    --k) K="$2"; shift 2 ;;
    --ell) ELL="$2"; shift 2 ;;
    --n) N="$2"; shift 2 ;;
    --server) SERVER_URL="$2"; shift 2 ;;
    --init) INIT_MODE="$2"; shift 2 ;;
    --base-port) BASE_PORT="$2"; shift 2 ;;
    --max-iters) MAX_ITERS="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOGDIR="$REPO/logs/fleet-$TIMESTAMP"
mkdir -p "$LOGDIR"

META="$LOGDIR/fleet.txt"
cat > "$META" <<EOF
Fleet: $NUM_WORKERS x $STRATEGY
Started:    $(date)
Target:     R($K,$ELL) n=$N
Init:       $INIT_MODE
Server:     $SERVER_URL
Max iters:  $MAX_ITERS
Base port:  $BASE_PORT
Logs:       $LOGDIR/
EOF

echo ""
echo "=========================================="
echo "  MineGraph Fleet: $NUM_WORKERS x $STRATEGY"
echo "=========================================="
echo ""
echo "  Target:     R($K,$ELL) n=$N"
echo "  Server:     $SERVER_URL"
echo "  Init:       $INIT_MODE"
echo "  Logs:       $LOGDIR/"
echo ""

# Build
echo "--- Building release binaries ---"
cargo build --release -p ramseynet-worker --quiet 2>&1

# Health check
if curl -sf "$SERVER_URL/api/health" > /dev/null 2>&1; then
  echo "--- Server healthy at $SERVER_URL ---"
else
  echo "--- WARNING: Server at $SERVER_URL not responding ---"
  echo "    Start it first: ./run server --release"
  echo ""
fi

# Track PIDs
PIDS=()
cleanup() {
  echo ""
  echo "--- Stopping $NUM_WORKERS workers ---"
  for pid in "${PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true

  echo ""
  echo "=========================================="
  echo "  Fleet Results"
  echo "=========================================="
  echo ""

  # Analyze each worker log
  total_rounds=0
  total_discoveries=0
  total_admitted=0
  total_submitted=0

  for i in $(seq 1 $NUM_WORKERS); do
    logfile="$LOGDIR/${STRATEGY}-${i}.log"
    last=$(grep 'round_summary' "$logfile" 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | tail -1 || true)
    if [ -n "$last" ]; then
      rounds=$(echo "$last" | grep -oP 'round=\K[0-9]+' || echo "0")
      disc=$(echo "$last" | grep -oP 'total_discoveries=\K[0-9]+' || echo "0")
      admit=$(echo "$last" | grep -oP 'total_admitted=\K[0-9]+' || echo "0")
      submit=$(echo "$last" | grep -oP 'total_submitted=\K[0-9]+' || echo "0")
      total_rounds=$((total_rounds + rounds))
      total_discoveries=$((total_discoveries + disc))
      total_admitted=$((total_admitted + admit))
      total_submitted=$((total_submitted + submit))
      printf "  Worker %2d: %5d rounds, %8d discoveries, %5d admitted\n" "$i" "$rounds" "$disc" "$admit"
    else
      printf "  Worker %2d: (no data)\n" "$i"
    fi
  done

  echo ""
  echo "  ────────────────────────────────────"
  echo "  Fleet totals:"
  echo "    Rounds:       $total_rounds"
  echo "    Discoveries:  $total_discoveries"
  echo "    Submitted:    $total_submitted"
  echo "    Admitted:     $total_admitted"
  if [ "$total_submitted" -gt 0 ]; then
    rate=$(awk "BEGIN {printf \"%.1f\", ($total_admitted / $total_submitted) * 100}")
    echo "    Admit rate:   ${rate}%"
  fi
  echo ""
  echo "  Logs: $LOGDIR/"
  echo ""
  echo "  To analyze:"
  echo "    ./scripts/analyze_experiment.sh $LOGDIR/"
  echo ""
  echo "=========================================="
}
trap cleanup EXIT INT TERM

# Launch workers
echo "--- Launching $NUM_WORKERS workers ---"
echo ""

for i in $(seq 1 $NUM_WORKERS); do
  port=$((BASE_PORT + i - 1))
  logfile="$LOGDIR/${STRATEGY}-${i}.log"

  RUST_LOG=info cargo run --release -p ramseynet-worker -- \
    --strategy "$STRATEGY" --k "$K" --ell "$ELL" --n "$N" \
    --server "$SERVER_URL" --init "$INIT_MODE" --port "$port" \
    --max-iters "$MAX_ITERS" \
    > "$logfile" 2>&1 &
  PIDS+=($!)
done

echo "  Dashboards:"
echo ""
for i in $(seq 1 $NUM_WORKERS); do
  port=$((BASE_PORT + i - 1))
  echo "    Worker $i:  http://localhost:$port"
done

echo ""
echo "  Open all dashboards:"
echo "    for p in $(seq $BASE_PORT $((BASE_PORT + NUM_WORKERS - 1)) | tr '\n' ' '); do xdg-open http://localhost:\$p; done"
echo ""
echo "=========================================="
echo "  Fleet running. Press Ctrl+C to stop."
echo "=========================================="
echo ""

# Wait
wait
