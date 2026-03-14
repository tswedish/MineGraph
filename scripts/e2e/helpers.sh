#!/usr/bin/env bash
# helpers.sh — Shared utilities for RamseyNet E2E tests (playwright-cli based)
# Source this file from test scripts: source "$(dirname "$0")/helpers.sh"

set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────
export E2E_BASE_URL="${E2E_BASE_URL:-http://localhost:5173}"
export E2E_API_URL="${E2E_API_URL:-http://localhost:3001}"
export E2E_SESSION="${E2E_SESSION:-ramseynet-e2e}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# playwright-cli writes --filename snapshots to $PWD
SNAPSHOT_DIR="$PROJECT_ROOT"

# Ensure we run from project root so snapshots land predictably
cd "$PROJECT_ROOT"

# ── Counters ────────────────────────────────────────────────────────
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
CURRENT_TEST=""
LAST_SNAPSHOT=""

# ── Colors ──────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# ── Logging ─────────────────────────────────────────────────────────
log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_pass()  { echo -e "${GREEN}[PASS]${NC}  $*"; }
log_fail()  { echo -e "${RED}[FAIL]${NC}  $*"; }
log_skip()  { echo -e "${YELLOW}[SKIP]${NC}  $*"; }
log_test()  { echo -e "\n${BOLD}── $* ──${NC}"; }
log_suite() { echo -e "\n${BOLD}══ $* ══${NC}"; }

# ── playwright-cli wrapper ──────────────────────────────────────────
# Runs playwright-cli with the test session name.
# Usage: pw <command> [args...]
pw() {
  playwright-cli -s="$E2E_SESSION" "$@" 2>&1
}

# ── Browser lifecycle ───────────────────────────────────────────────
browser_open() {
  local url="${1:-$E2E_BASE_URL}"
  log_info "Opening browser -> $url"
  pw open --browser=chromium "$url" || true
  sleep 3  # Let the page fully render + API calls settle
}

browser_close() {
  log_info "Closing browser"
  pw close 2>/dev/null || true
}

# ── Navigation ──────────────────────────────────────────────────────
# Tracks last page URL and title from pw output.
LAST_PAGE_URL=""
LAST_PAGE_TITLE=""

navigate() {
  local url="$1"
  log_info "Navigating -> $url"
  local output
  output=$(pw goto "$url" 2>&1 || true)
  sleep 3  # Let SPA routing + API fetches complete
  # Extract URL and title from pw output
  LAST_PAGE_URL=$(echo "$output" | grep "Page URL:" | tail -1 | sed 's/.*Page URL: //' | tr -d '[:space:]')
  LAST_PAGE_TITLE=$(echo "$output" | grep "Page Title:" | tail -1 | sed 's/.*Page Title: //')
}

# ── Snapshot capture ────────────────────────────────────────────────
# Take a snapshot and store path in LAST_SNAPSHOT.
# Also updates LAST_PAGE_URL and LAST_PAGE_TITLE from pw output.
snap() {
  local filename="snap-$(date +%s%N).yml"
  local output
  output=$(pw snapshot --filename="$filename" 2>&1 || true)
  LAST_SNAPSHOT="$SNAPSHOT_DIR/$filename"
  
  # Extract URL and title from snapshot output
  local url_line title_line
  url_line=$(echo "$output" | grep "Page URL:" | tail -1 || true)
  title_line=$(echo "$output" | grep "Page Title:" | tail -1 || true)
  if [[ -n "$url_line" ]]; then
    LAST_PAGE_URL=$(echo "$url_line" | sed 's/.*Page URL: //' | tr -d '[:space:]')
  fi
  if [[ -n "$title_line" ]]; then
    LAST_PAGE_TITLE=$(echo "$title_line" | sed 's/.*Page Title: //')
  fi
  
  # Wait briefly for file to be written
  sleep 0.5
  
  if [[ ! -f "$LAST_SNAPSHOT" ]]; then
    log_fail "Snapshot file not created: $LAST_SNAPSHOT"
    return 1
  fi
  return 0
}

# ── Assertions ──────────────────────────────────────────────────────

# Assert that the last snapshot contains a text string (case-insensitive).
# Usage: assert_text "RamseyNet" "homepage title"
assert_text() {
  local text="$1"
  local description="${2:-contains '$text'}"
  CURRENT_TEST="$description"

  if [[ ! -f "$LAST_SNAPSHOT" ]]; then
    log_fail "$description -- no snapshot file"
    ((FAIL_COUNT++)) || true
    return 1
  fi

  if grep -qi "$text" "$LAST_SNAPSHOT" 2>/dev/null; then
    log_pass "$description"
    ((PASS_COUNT++)) || true
    return 0
  else
    log_fail "$description -- '$text' not found in snapshot"
    ((FAIL_COUNT++)) || true
    return 1
  fi
}

# Assert that the last snapshot does NOT contain a text string.
# Usage: assert_no_text "error" "no errors on page"
assert_no_text() {
  local text="$1"
  local description="${2:-does not contain '$text'}"
  CURRENT_TEST="$description"

  if [[ ! -f "$LAST_SNAPSHOT" ]]; then
    log_fail "$description -- no snapshot file"
    ((FAIL_COUNT++)) || true
    return 1
  fi

  if grep -qi "$text" "$LAST_SNAPSHOT" 2>/dev/null; then
    log_fail "$description -- '$text' was found in snapshot"
    ((FAIL_COUNT++)) || true
    return 1
  else
    log_pass "$description"
    ((PASS_COUNT++)) || true
    return 0
  fi
}

# Assert the page URL contains a substring.
# Uses LAST_PAGE_URL captured from pw output (not from snapshot file).
# Usage: assert_url "/leaderboards" "on leaderboards page"
assert_url() {
  local expected="$1"
  local description="${2:-URL contains '$expected'}"
  CURRENT_TEST="$description"

  if echo "$LAST_PAGE_URL" | grep -qi "$expected" 2>/dev/null; then
    log_pass "$description"
    ((PASS_COUNT++)) || true
    return 0
  else
    log_fail "$description -- URL '$LAST_PAGE_URL' does not contain '$expected'"
    ((FAIL_COUNT++)) || true
    return 1
  fi
}

# Assert the page title contains a substring.
# Uses LAST_PAGE_TITLE captured from pw output (not from snapshot file).
# Usage: assert_title "RamseyNet" "page title"
assert_title() {
  local expected="$1"
  local description="${2:-title contains '$expected'}"
  CURRENT_TEST="$description"

  if echo "$LAST_PAGE_TITLE" | grep -qi "$expected" 2>/dev/null; then
    log_pass "$description"
    ((PASS_COUNT++)) || true
    return 0
  else
    log_fail "$description -- title '$LAST_PAGE_TITLE' does not contain '$expected'"
    ((FAIL_COUNT++)) || true
    return 1
  fi
}

# ── Element interaction helpers ─────────────────────────────────────

# Find a ref (e.g., "e15") for an element containing the given text.
# Searches the last snapshot. Returns the first match.
# Usage: ref=$(find_ref "Submit Graph")
find_ref() {
  local text="$1"
  if [[ ! -f "$LAST_SNAPSHOT" ]]; then
    return 1
  fi
  grep -i "$text" "$LAST_SNAPSHOT" 2>/dev/null \
    | grep -oP '\be\d+\b' \
    | head -1 || true
}

# Find all refs matching text (returns newline-separated).
find_refs() {
  local text="$1"
  if [[ ! -f "$LAST_SNAPSHOT" ]]; then
    return 1
  fi
  grep -i "$text" "$LAST_SNAPSHOT" 2>/dev/null \
    | grep -oP '\be\d+\b' \
    | sort -u || true
}

# Click an element found by text in the snapshot.
# Usage: click_text "Leaderboards"
click_text() {
  local text="$1"
  log_info "Looking for clickable element: '$text'"

  local ref
  ref=$(find_ref "$text")

  if [[ -z "$ref" ]]; then
    log_fail "Could not find clickable element with text '$text'"
    return 1
  fi

  log_info "Found ref: $ref -- clicking"
  local output
  output=$(pw click "$ref" 2>&1 || true)
  sleep 3  # Let navigation/fetch settle
  # Update URL/title if click triggered navigation
  local url_line title_line
  url_line=$(echo "$output" | grep "Page URL:" | tail -1 || true)
  title_line=$(echo "$output" | grep "Page Title:" | tail -1 || true)
  if [[ -n "$url_line" ]]; then
    LAST_PAGE_URL=$(echo "$url_line" | sed 's/.*Page URL: //' | tr -d '[:space:]')
  fi
  if [[ -n "$title_line" ]]; then
    LAST_PAGE_TITLE=$(echo "$title_line" | sed 's/.*Page Title: //')
  fi
}

# Click a specific ref directly.
# Usage: click_ref "e15"
click_ref() {
  local ref="$1"
  log_info "Clicking ref $ref"
  local output
  output=$(pw click "$ref" 2>&1 || true)
  sleep 3
  # Update URL/title if click triggered navigation
  local url_line title_line
  url_line=$(echo "$output" | grep "Page URL:" | tail -1 || true)
  title_line=$(echo "$output" | grep "Page Title:" | tail -1 || true)
  if [[ -n "$url_line" ]]; then
    LAST_PAGE_URL=$(echo "$url_line" | sed 's/.*Page URL: //' | tr -d '[:space:]')
  fi
  if [[ -n "$title_line" ]]; then
    LAST_PAGE_TITLE=$(echo "$title_line" | sed 's/.*Page Title: //')
  fi
}

# Fill a form field by ref.
# Usage: fill_ref "e5" "some value"
fill_ref() {
  local ref="$1"
  local value="$2"
  log_info "Filling $ref with value"
  pw fill "$ref" "$value" >/dev/null 2>&1 || true
  sleep 1
}

# Type text (keyboard input, not targeted).
# Usage: type_text "some text"
type_text() {
  local text="$1"
  log_info "Typing text"
  pw type "$text" || true
  sleep 1
}

# ── Server health checks ───────────────────────────────────────────

wait_for_api() {
  local max_wait="${1:-30}"
  log_info "Waiting for API server at $E2E_API_URL (max ${max_wait}s)..."
  local i=0
  while [[ $i -lt $max_wait ]]; do
    if curl -sf "$E2E_API_URL/api/health" >/dev/null 2>&1; then
      log_info "API server is ready"
      return 0
    fi
    sleep 1
    ((i++))
  done
  log_fail "API server did not start within ${max_wait}s"
  return 1
}

wait_for_web() {
  local max_wait="${1:-30}"
  log_info "Waiting for web server at $E2E_BASE_URL (max ${max_wait}s)..."
  local i=0
  while [[ $i -lt $max_wait ]]; do
    if curl -sf "$E2E_BASE_URL" >/dev/null 2>&1; then
      log_info "Web server is ready"
      return 0
    fi
    sleep 1
    ((i++))
  done
  log_fail "Web server did not start within ${max_wait}s"
  return 1
}

# ── Summary ─────────────────────────────────────────────────────────

print_summary() {
  local total=$((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))
  echo ""
  echo -e "${BOLD}==========================================${NC}"
  echo -e "${BOLD} E2E Test Summary${NC}"
  echo -e "${BOLD}==========================================${NC}"
  echo -e "  ${GREEN}Passed:${NC}  $PASS_COUNT"
  echo -e "  ${RED}Failed:${NC}  $FAIL_COUNT"
  echo -e "  ${YELLOW}Skipped:${NC} $SKIP_COUNT"
  echo -e "  Total:   $total"
  echo -e "${BOLD}==========================================${NC}"

  if [[ $FAIL_COUNT -gt 0 ]]; then
    echo -e "${RED}${BOLD}TESTS FAILED${NC}"
    return 1
  else
    echo -e "${GREEN}${BOLD}ALL TESTS PASSED${NC}"
    return 0
  fi
}
