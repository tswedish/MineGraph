#!/usr/bin/env bash
# e2e-test.sh — Main orchestrator for RamseyNet E2E tests
#
# Usage:
#   ./scripts/e2e-test.sh              # Full suite (start servers, seed, test, cleanup)
#   ./scripts/e2e-test.sh --no-server  # Tests only (assumes servers already running)
#   ./scripts/e2e-test.sh --test homepage          # Run one test suite
#   ./scripts/e2e-test.sh --test submit-form       # Run one test suite
#
# Environment:
#   E2E_BASE_URL   Web server URL (default: http://localhost:5173)
#   E2E_API_URL    API server URL (default: http://localhost:3001)
#   E2E_SESSION    Playwright session name (default: ramseynet-e2e)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Colors ──────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# ── Parse args ──────────────────────────────────────────────────────
MANAGE_SERVERS=true
SINGLE_TEST=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-server)
      MANAGE_SERVERS=false
      shift
      ;;
    --test)
      SINGLE_TEST="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1"
      echo "Usage: $0 [--no-server] [--test <name>]"
      exit 1
      ;;
  esac
done

# ── Config ──────────────────────────────────────────────────────────
export E2E_BASE_URL="${E2E_BASE_URL:-http://localhost:5173}"
export E2E_API_URL="${E2E_API_URL:-http://localhost:3001}"
export E2E_SESSION="${E2E_SESSION:-ramseynet-e2e}"

API_PID=""
WEB_PID=""

# ── Cleanup on exit ────────────────────────────────────────────────
cleanup() {
  echo -e "\n${BLUE}[INFO]${NC}  Cleaning up..."

  # Close browser
  playwright-cli -s="$E2E_SESSION" close 2>/dev/null || true

  # Kill background servers if we started them
  if [[ "$MANAGE_SERVERS" == "true" ]]; then
    if [[ -n "$WEB_PID" ]] && kill -0 "$WEB_PID" 2>/dev/null; then
      echo -e "${BLUE}[INFO]${NC}  Stopping web server (PID $WEB_PID)"
      kill "$WEB_PID" 2>/dev/null || true
      wait "$WEB_PID" 2>/dev/null || true
    fi
    if [[ -n "$API_PID" ]] && kill -0 "$API_PID" 2>/dev/null; then
      echo -e "${BLUE}[INFO]${NC}  Stopping API server (PID $API_PID)"
      kill "$API_PID" 2>/dev/null || true
      wait "$API_PID" 2>/dev/null || true
    fi
  fi

  echo -e "${BLUE}[INFO]${NC}  Cleanup complete"
}
trap cleanup EXIT

# ── Prerequisites ───────────────────────────────────────────────────
echo -e "${BOLD}===========================================${NC}"
echo -e "${BOLD} RamseyNet E2E Test Suite${NC}"
echo -e "${BOLD}===========================================${NC}"
echo ""

check_prereq() {
  if ! command -v "$1" &>/dev/null; then
    echo -e "${RED}[ERROR]${NC} Required tool not found: $1"
    echo "  $2"
    exit 1
  fi
}

check_prereq "playwright-cli" "Install: npm install -g playwright-cli"
check_prereq "curl" "Install via your system package manager"

if [[ "$MANAGE_SERVERS" == "true" ]]; then
  check_prereq "cargo" "Install Rust: https://rustup.rs"
  check_prereq "pnpm" "Install pnpm: npm install -g pnpm"
fi

echo -e "${GREEN}[OK]${NC}    All prerequisites found"

# ── Start servers (if managed) ──────────────────────────────────────
if [[ "$MANAGE_SERVERS" == "true" ]]; then
  echo ""
  echo -e "${BOLD}--- Starting servers ---${NC}"

  # Clean database for fresh state
  echo -e "${BLUE}[INFO]${NC}  Cleaning database for fresh test state..."
  rm -f "$PROJECT_ROOT/ramseynet.db" \
        "$PROJECT_ROOT/ramseynet.db-wal" \
        "$PROJECT_ROOT/ramseynet.db-shm"

  # Build the server
  echo -e "${BLUE}[INFO]${NC}  Building API server..."
  cargo build -p ramseynet-server 2>&1 | tail -1

  # Start API server in background
  echo -e "${BLUE}[INFO]${NC}  Starting API server on :3001..."
  cargo run -p ramseynet-server -- --port 3001 \
    >"$PROJECT_ROOT/logs/e2e-server.log" 2>&1 &
  API_PID=$!
  echo -e "${BLUE}[INFO]${NC}  API server PID: $API_PID"

  # Wait for API server
  echo -e "${BLUE}[INFO]${NC}  Waiting for API server..."
  for i in $(seq 1 30); do
    if curl -sf "http://localhost:3001/api/health" >/dev/null 2>&1; then
      echo -e "${GREEN}[OK]${NC}    API server is ready"
      break
    fi
    if [[ $i -eq 30 ]]; then
      echo -e "${RED}[ERROR]${NC} API server failed to start"
      cat "$PROJECT_ROOT/logs/e2e-server.log" | tail -20
      exit 1
    fi
    sleep 1
  done

  # Install web dependencies + start dev server
  echo -e "${BLUE}[INFO]${NC}  Installing web dependencies..."
  (cd "$PROJECT_ROOT/web" && pnpm install --silent 2>&1) || true

  echo -e "${BLUE}[INFO]${NC}  Starting web dev server on :5173..."
  (cd "$PROJECT_ROOT/web" && pnpm dev) >"$PROJECT_ROOT/logs/e2e-web.log" 2>&1 &
  WEB_PID=$!
  echo -e "${BLUE}[INFO]${NC}  Web server PID: $WEB_PID"

  # Wait for web server
  echo -e "${BLUE}[INFO]${NC}  Waiting for web server..."
  for i in $(seq 1 30); do
    if curl -sf "http://localhost:5173" >/dev/null 2>&1; then
      echo -e "${GREEN}[OK]${NC}    Web server is ready"
      break
    fi
    if [[ $i -eq 30 ]]; then
      echo -e "${RED}[ERROR]${NC} Web server failed to start"
      cat "$PROJECT_ROOT/logs/e2e-web.log" | tail -20
      exit 1
    fi
    sleep 1
  done

  # Seed test data
  echo -e "${BLUE}[INFO]${NC}  Seeding test data..."
  bash "$PROJECT_ROOT/scripts/seed-ledger.sh" >/dev/null 2>&1
  echo -e "${GREEN}[OK]${NC}    Test data seeded"

else
  # Verify servers are running
  echo -e "${BLUE}[INFO]${NC}  Checking existing servers..."
  if ! curl -sf "$E2E_API_URL/api/health" >/dev/null 2>&1; then
    echo -e "${RED}[ERROR]${NC} API server not responding at $E2E_API_URL"
    echo "  Start it: ./run server"
    exit 1
  fi
  if ! curl -sf "$E2E_BASE_URL" >/dev/null 2>&1; then
    echo -e "${RED}[ERROR]${NC} Web server not responding at $E2E_BASE_URL"
    echo "  Start it: ./run web-dev"
    exit 1
  fi
  echo -e "${GREEN}[OK]${NC}    Both servers are running"
fi

# ── Open browser ────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}--- Opening browser ---${NC}"
cd "$PROJECT_ROOT"
playwright-cli -s="$E2E_SESSION" open --browser=chromium "$E2E_BASE_URL" 2>/dev/null || true
sleep 3
echo -e "${GREEN}[OK]${NC}    Browser opened"

# ── Run tests ───────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}--- Running tests ---${NC}"

TOTAL_PASS=0
TOTAL_FAIL=0
TOTAL_SKIP=0
SUITE_RESULTS=""

run_test_suite() {
  local name="$1"
  local script="$SCRIPT_DIR/e2e/test-${name}.sh"

  if [[ ! -f "$script" ]]; then
    echo -e "${RED}[ERROR]${NC} Test script not found: $script"
    return 1
  fi

  echo ""
  echo -e "${BOLD}>>> Running: test-${name}.sh <<<${NC}"

  # Run the test in a subshell to capture its exit code
  # but don't exit the orchestrator on failure
  local result=0
  bash "$script" || result=$?

  return $result
}

if [[ -n "$SINGLE_TEST" ]]; then
  # Run single test suite
  run_test_suite "$SINGLE_TEST" || true
else
  # Run all test suites in order
  SUITES=("homepage" "leaderboards" "submission" "submit-form" "navigation")

  for suite in "${SUITES[@]}"; do
    run_test_suite "$suite" || true
  done
fi

# ── Final summary ──────────────────────────────────────────────────
echo ""
echo -e "${BOLD}===========================================${NC}"
echo -e "${BOLD} E2E Test Suite Complete${NC}"
echo -e "${BOLD}===========================================${NC}"
echo ""
echo "Server logs: logs/e2e-server.log, logs/e2e-web.log"
echo "Snapshots:   .playwright-cli/snap-*.yml"
echo ""
echo "To re-run with existing servers:"
echo "  ./scripts/e2e-test.sh --no-server"
echo ""
echo "To run a single suite:"
echo "  ./scripts/e2e-test.sh --no-server --test homepage"
