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
snap

# Find the K spinbutton (accessible name is exactly "K")
K_REF=$(find_ref 'spinbutton "K"' || true)
if [[ -n "$K_REF" ]]; then
  fill_ref "$K_REF" "4"
fi

# Find the L spinbutton
snap
L_REF=$(find_ref 'spinbutton "L"' || true)
if [[ -n "$L_REF" ]]; then
  fill_ref "$L_REF" "4"
fi

# Find the RGXF textarea (shows as textbox "RGXF JSON" in accessibility tree)
RGXF_JSON='{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}'
snap
TEXTAREA_REF=$(find_ref 'textbox "RGXF' || true)
if [[ -n "$TEXTAREA_REF" ]]; then
  fill_ref "$TEXTAREA_REF" "$RGXF_JSON"
  sleep 2
fi

snap

# Check that preview appeared (MatrixView canvas has aria-label)
assert_text "Adjacency matrix" "Live preview matrix rendered after RGXF paste"

# Find and click Submit button
SUBMIT_REF=$(find_ref "Submit Graph" || find_ref "Submit graph" || true)
if [[ -n "$SUBMIT_REF" ]]; then
  click_ref "$SUBMIT_REF"
  sleep 2
fi

snap

# Verify accepted result
assert_text "accepted" "Result shows 'accepted' verdict"

# ── Test 4.4: Submit rejected graph (K5 for R(3,3)) ────────────────
log_test "4.4 Submit rejected graph"
navigate "$E2E_BASE_URL/submit"
sleep 2
snap

# Fill K=3
K_REF=$(find_ref 'spinbutton "K"' || true)
if [[ -n "$K_REF" ]]; then
  fill_ref "$K_REF" "3"
fi

# Fill L=3
snap
L_REF=$(find_ref 'spinbutton "L"' || true)
if [[ -n "$L_REF" ]]; then
  fill_ref "$L_REF" "3"
fi

# Paste K5 (complete graph) RGXF — this should be rejected (has 3-clique)
K5_JSON='{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "/8A="}'
snap
TEXTAREA_REF=$(find_ref 'textbox "RGXF' || true)
if [[ -n "$TEXTAREA_REF" ]]; then
  fill_ref "$TEXTAREA_REF" "$K5_JSON"
  sleep 2
fi

# Click submit
snap
SUBMIT_REF=$(find_ref "Submit Graph" || find_ref "Submit graph" || true)
if [[ -n "$SUBMIT_REF" ]]; then
  click_ref "$SUBMIT_REF"
  sleep 2
fi

snap

# Verify rejected result with reason
assert_text "rejected" "Result shows 'rejected' verdict"
assert_text "clique" "Reason mentions clique"

# ── Test 4.5: Invalid JSON input ────────────────────────────────────
log_test "4.5 Invalid JSON handling"
navigate "$E2E_BASE_URL/submit"
sleep 2
snap

# Find textarea and type invalid JSON
TEXTAREA_REF=$(find_ref 'textbox "RGXF' || true)
if [[ -n "$TEXTAREA_REF" ]]; then
  fill_ref "$TEXTAREA_REF" "not valid json at all"
  sleep 2
fi

snap

# Should show a parse error — the SubmitForm shows "Invalid JSON" on bad input
assert_text "Invalid" "Parse error shown for invalid JSON"

# ── Summary ─────────────────────────────────────────────────────────
print_summary
