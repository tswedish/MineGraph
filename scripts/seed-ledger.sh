#!/usr/bin/env bash
# Seed the RamseyNet ledger with sample graphs.
# Usage: bash scripts/seed-ledger.sh [BASE_URL]
#
# Requires: curl, jq (optional, for pretty output)
# Default: http://localhost:3001

set -euo pipefail

BASE="${1:-http://localhost:3001}"
echo "=== Seeding RamseyNet at $BASE ==="

# ── Helper ──────────────────────────────────────────────────────────
post() {
  local endpoint="$1"
  local data="$2"
  local label="$3"
  local resp
  resp=$(curl -s -w "\n%{http_code}" -X POST "$BASE/api/$endpoint" \
    -H "Content-Type: application/json" \
    -d "$data")
  local code=$(echo "$resp" | tail -1)
  local body=$(echo "$resp" | sed '$d')
  if command -v jq &>/dev/null; then
    echo "[$code] $label"
    echo "$body" | jq . 2>/dev/null || echo "$body"
  else
    echo "[$code] $label: $body"
  fi
  echo ""
}

# ── Submit graphs directly (no challenge creation needed) ──────────
echo "--- Submitting graphs ---"

# C5 (5-cycle) for R(3,3) n=5: edges 0-1,1-2,2-3,3-4,4-0
# omega=2, alpha=2 → accepted
# bits: (0,1)=1 (0,2)=0 (0,3)=0 (0,4)=1 (1,2)=1 (1,3)=0 (1,4)=0 (2,3)=1 (2,4)=0 (3,4)=1
# binary: 10011001 01 → 0x99 0x40 → base64: mUA=
post submit \
  '{"k":3,"ell":3,"n":5,"graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"mUA="}}' \
  "C5 → R(3,3) n=5 [expect: accepted, admitted]"

# Wagner graph (8 vertices) for R(3,4) n=8: circulant C(8, {1, 4})
# 3-regular, triangle-free — omega=2, alpha=3
# omega < 3 ✓, alpha < 4 ✓ → accepted
post submit \
  '{"k":3,"ell":4,"n":8,"graph":{"n":8,"encoding":"utri_b64_v1","bits_b64":"kySmUA=="}}' \
  "Wagner graph → R(3,4) n=8 [expect: accepted, admitted]"

# K5 (complete on 5 vertices) for R(3,3) n=5 — should be REJECTED (has triangle)
# All 10 upper-tri bits = 1 → 11111111 11 → 0xFF 0xC0 → base64: /8A=
post submit \
  '{"k":3,"ell":3,"n":5,"graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"/8A="}}' \
  "K5 → R(3,3) n=5 [expect: rejected, clique_found]"

# Empty graph on 5 vertices for R(3,3) n=5 — should be REJECTED (has independent set)
# All bits = 0 → 0x00 0x00 → base64: AAA=
post submit \
  '{"k":3,"ell":3,"n":5,"graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"AAA="}}' \
  "E5 → R(3,3) n=5 [expect: rejected, independent_set_found]"

echo "=== Seeding complete ==="
echo ""
echo "Open http://localhost:5173 to explore the web app."
echo "Leaderboards should be visible at /leaderboards."
