#!/usr/bin/env bash
# test-navigation.sh — E2E tests for navigation and routing
source "$(dirname "$0")/helpers.sh"

log_suite "Navigation Tests"

# ── Test 5.1: Nav links from homepage ──────────────────────────────
log_test "5.1 Nav links from homepage"
navigate "$E2E_BASE_URL"
snap

# Click Leaderboards nav link
click_text "Leaderboards"
snap

assert_url "/leaderboards" "Leaderboards nav link works"

# ── Test 5.2: Submit nav link ──────────────────────────────────────
log_test "5.2 Submit nav link"
click_text "Submit"
snap

assert_url "/submit" "Submit nav link works"

# ── Test 5.3: Logo returns to homepage ─────────────────────────────
log_test "5.3 Logo -> homepage"
# Click the RamseyNet logo/brand link in the nav
click_text "RamseyNet"
snap

assert_url "5173" "Logo link returns to homepage (root URL)"
assert_text "RamseyNet" "Homepage content visible"

# ── Test 5.4: Direct URL access ────────────────────────────────────
log_test "5.4 Direct URL access"
# SPA with fallback: 'index.html' should handle direct deep links
navigate "$E2E_BASE_URL/leaderboards/3/3/5"
snap

assert_url "/leaderboards/3/3/5" "Direct URL /leaderboards/3/3/5 loads"
assert_text "3,3" "Direct URL shows correct content"

# ── Test 5.5: Browser back navigation ──────────────────────────────
log_test "5.5 Browser back"
navigate "$E2E_BASE_URL/leaderboards"
sleep 1
navigate "$E2E_BASE_URL/submit"
sleep 1

# Go back
pw go-back || true
sleep 3
snap

assert_url "/leaderboards" "Browser back returns to leaderboards"

# ── Test 5.6: 404 / unknown route handling ─────────────────────────
log_test "5.6 Unknown route"
# Navigate via run-code to avoid navigate() blocking on error pages
pw run-code "await page.goto('$E2E_BASE_URL/nonexistent-page', { waitUntil: 'domcontentloaded', timeout: 10000 }).catch(() => {})" >/dev/null 2>&1 || true
sleep 3
snap

# Custom +error.svelte renders with the app layout (nav + error content)
# The nav still contains "RamseyNet" and the error page shows the status code
assert_text "RamseyNet" "Unknown route still loads SPA shell with nav"
assert_text "404" "Error page shows 404 status"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
