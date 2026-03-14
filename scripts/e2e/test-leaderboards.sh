#!/usr/bin/env bash
# test-leaderboards.sh — E2E tests for leaderboards index + detail pages
source "$(dirname "$0")/helpers.sh"

log_suite "Leaderboard Tests"

# ── Test 2.1: Leaderboards index loads ──────────────────────────────
log_test "2.1 Leaderboards index page"
navigate "$E2E_BASE_URL/leaderboards"
snap

assert_url "/leaderboards" "URL is /leaderboards"
assert_text "Leaderboard" "Page has leaderboard heading"

# ── Test 2.2: Seeded (K,L) pairs appear ────────────────────────────
log_test "2.2 Seeded data visible"
# After seeding, we should see R(3,3) and R(3,4)
assert_text "3,3" "R(3,3) pair visible"
assert_text "3,4" "R(3,4) pair visible"

# ── Test 2.3: N-value chips with entry counts ──────────────────────
log_test "2.3 N-value chips"
# The seeded data has n=5 for R(3,3) and n=8 for R(3,4)
assert_text "5" "n=5 chip visible"
assert_text "8" "n=8 chip visible"

# ── Test 2.4: Click into leaderboard detail ─────────────────────────
log_test "2.4 Navigate to leaderboard detail"
# Navigate directly to R(3,3) n=5
navigate "$E2E_BASE_URL/leaderboards/3/3/5"
snap

assert_url "/leaderboards/3/3/5" "URL is /leaderboards/3/3/5"
assert_text "3,3" "Header shows R(3,3)"
assert_text "5" "Header shows n=5"

# ── Test 2.5: Leaderboard detail has ranked table ──────────────────
log_test "2.5 Ranked table"
# The table should show rank, CID columns, score columns
assert_text "#" "Rank column header visible"

# ── Test 2.6: Score columns present ────────────────────────────────
log_test "2.6 Score columns"
# C_max and C_min columns (clique counts)
# These are the tier-1 scoring columns
assert_text "Aut" "Automorphism column visible"

# ── Test 2.7: Top graph visualization ──────────────────────────────
log_test "2.7 Top graph visualization"
# MatrixView canvas has aria-label; CircleLayout SVG renders as img
assert_text "Adjacency matrix" "MatrixView canvas present"

# ── Test 2.8: Navigate to R(3,4) n=8 ──────────────────────────────
log_test "2.8 R(3,4) n=8 leaderboard"
navigate "$E2E_BASE_URL/leaderboards/3/4/8"
snap

assert_url "/leaderboards/3/4/8" "URL is /leaderboards/3/4/8"
assert_text "3,4" "Header shows R(3,4)"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
