#!/usr/bin/env bash
# test-submission.sh — E2E tests for the submission detail page
source "$(dirname "$0")/helpers.sh"

log_suite "Submission Detail Tests"

# ── Get a real CID from the API ─────────────────────────────────────
log_test "3.0 Fetch a CID from leaderboard"
CID=$(curl -sf "$E2E_API_URL/api/leaderboards/3/3/5" 2>/dev/null \
  | grep -oP '"graph_cid"\s*:\s*"[0-9a-f]+"' \
  | head -1 \
  | grep -oP '[0-9a-f]{64}' || true)

if [[ -z "$CID" ]]; then
  log_skip "No CID found in leaderboard -- skipping submission tests"
  SKIP_COUNT=$((SKIP_COUNT + 6))
  print_summary
  exit 0
fi

log_info "Using CID: ${CID:0:16}..."

# ── Test 3.1: Submission detail page loads ──────────────────────────
log_test "3.1 Submission detail page"
navigate "$E2E_BASE_URL/submissions/$CID"
snap

assert_url "/submissions/" "URL contains /submissions/"

# ── Test 3.2: CID is displayed ──────────────────────────────────────
log_test "3.2 CID displayed"
# Check that at least the first 16 chars of the CID appear
CID_PREFIX="${CID:0:16}"
assert_text "$CID_PREFIX" "CID prefix visible on page"

# ── Test 3.3: Verdict badge ─────────────────────────────────────────
log_test "3.3 Verdict badge"
# The C5 graph should be accepted for R(3,3)
assert_text "accepted" "Verdict shows 'accepted'"

# ── Test 3.4: Ramsey params link ────────────────────────────────────
log_test "3.4 Ramsey parameters"
assert_text "3,3" "R(3,3) parameters shown"

# ── Test 3.5: Graph visualization ──────────────────────────────────
log_test "3.5 Graph visualization"
# MatrixView canvas has aria-label; CircleLayout SVG renders as img
assert_text "Adjacency matrix" "MatrixView canvas present"

# ── Test 3.6: Timestamps ──────────────────────────────────────────
log_test "3.6 Metadata present"
# Submission should have timestamp info
assert_text "202" "Timestamp year visible (2025/2026)"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
