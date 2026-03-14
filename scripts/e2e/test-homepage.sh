#!/usr/bin/env bash
# test-homepage.sh — E2E tests for the RamseyNet homepage
source "$(dirname "$0")/helpers.sh"

log_suite "Homepage Tests"

# ── Test 1.1: Page loads ────────────────────────────────────────────
log_test "1.1 Homepage loads"
navigate "$E2E_BASE_URL"
snap

assert_title "RamseyNet" "Page title contains 'RamseyNet'"
assert_text "RamseyNet" "Page has 'RamseyNet' heading"

# ── Test 1.2: Health badge ──────────────────────────────────────────
log_test "1.2 Health badge"
# The health badge fetches /api/health and shows server status
assert_text "ok" "Health badge shows 'ok' status"

# ── Test 1.3: Navigation cards ──────────────────────────────────────
log_test "1.3 Navigation cards"
assert_text "Leaderboards" "Leaderboards card/link visible"
assert_text "Submit" "Submit card/link visible"

# ── Test 1.4: No error state ───────────────────────────────────────
log_test "1.4 No errors"
assert_no_text "Error" "No error messages on homepage"
assert_no_text "failed" "No failure messages on homepage"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
