#!/usr/bin/env bash
# test-submit-form.sh — E2E tests for the graph submission form
source "$(dirname "$0")/helpers.sh"

log_suite "Submit Form Tests"

# ── Test 4.1: Submit page loads ─────────────────────────────────────
log_test "4.1 Submit page loads"
navigate "$E2E_BASE_URL/submit"
snap

assert_url "/submit" "URL is /submit"
assert_text "Submit" "Page has Submit heading"

# ── Test 4.2: Form inputs visible ──────────────────────────────────
log_test "4.2 Form inputs present"
assert_text "spinbutton" "Number inputs present"
assert_text "RGXF" "RGXF JSON input visible"

# ── Test 4.3: Submit accepted graph (C5 for R(4,4)) ────────────────
log_test "4.3 Submit accepted graph"
navigate "$E2E_BASE_URL/submit"
sleep 1

# Fill form using Playwright role-based locators (no snapshot-ref needed)
fill_by_role "spinbutton" "K" "4"
fill_by_role "spinbutton" "L" "4"

RGXF_JSON='{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}'
fill_by_role "textbox" "RGXF JSON" "$RGXF_JSON"
sleep 2

snap

# Check that preview appeared (MatrixView canvas has aria-label)
assert_text "Adjacency matrix" "Live preview matrix rendered after RGXF paste"

# Click submit
click_by_role "button" "Submit Graph"
sleep 2

snap

# Verify accepted result
assert_text "accepted" "Result shows 'accepted' verdict"

# ── Test 4.4: Submit rejected graph (K5 for R(3,3)) ────────────────
log_test "4.4 Submit rejected graph"
navigate "$E2E_BASE_URL/submit"
sleep 2

fill_by_role "spinbutton" "K" "3"
fill_by_role "spinbutton" "L" "3"

K5_JSON='{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "/8A="}'
fill_by_role "textbox" "RGXF JSON" "$K5_JSON"
sleep 2

click_by_role "button" "Submit Graph"
sleep 2

snap

# Verify rejected result with reason
assert_text "rejected" "Result shows 'rejected' verdict"
assert_text "clique" "Reason mentions clique"

# ── Test 4.5: Invalid JSON input ────────────────────────────────────
log_test "4.5 Invalid JSON handling"
navigate "$E2E_BASE_URL/submit"
sleep 2

fill_by_role "textbox" "RGXF JSON" "not valid json at all"
sleep 2

snap

# Should show a parse error — the SubmitForm shows "Invalid JSON" on bad input
assert_text "Invalid" "Parse error shown for invalid JSON"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
