#!/usr/bin/env bash
# Seed the RamseyNet ledger with sample challenges and graphs.
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

# ── 1. Create challenges ────────────────────────────────────────────
echo "--- Creating challenges ---"

post challenges \
  '{"k":3,"ell":3,"description":"Classic R(3,3)=6 — find the largest 2-coloring of K_n with no monochromatic triangle"}' \
  "R(3,3)"

post challenges \
  '{"k":3,"ell":4,"description":"R(3,4)=9 — no red triangle or blue K4"}' \
  "R(3,4)"

post challenges \
  '{"k":4,"ell":4,"description":"R(4,4)=18 — no monochromatic K4 (hard!)"}' \
  "R(4,4)"

# ── 2. Submit known-good graphs ─────────────────────────────────────
echo "--- Submitting graphs ---"

# C5 (5-cycle) for R(3,3): edges 0-1,1-2,2-3,3-4,4-0
# omega=2, alpha=2 → accepted
# bits: (0,1)=1 (0,2)=0 (0,3)=0 (0,4)=1 (1,2)=1 (1,3)=0 (1,4)=0 (2,3)=1 (2,4)=0 (3,4)=1
# binary: 10011001 01 → 0x99 0x40 → base64: mUA=
post submit \
  '{"challenge_id":"ramsey:3:3:v1","graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"mUA="}}' \
  "C5 → R(3,3) [expect: accepted, n=5]"

# Petersen graph (10 vertices) for R(3,4): omega=2, alpha=4 → k-clique < 3, ell-indep < 4
# 3-regular, girth 5 — should be accepted for R(3,4)
# Edges: 0-1,0-4,0-5, 1-2,1-6, 2-3,2-7, 3-4,3-8, 4-9, 5-7,5-8, 6-8,6-9, 7-9
# Upper-tri pairs for n=10: 45 bits
# Row 0: (0,1)=1 (0,2)=0 (0,3)=0 (0,4)=1 (0,5)=1 (0,6)=0 (0,7)=0 (0,8)=0 (0,9)=0
# Row 1: (1,2)=1 (1,3)=0 (1,4)=0 (1,5)=0 (1,6)=1 (1,7)=0 (1,8)=0 (1,9)=0
# Row 2: (2,3)=1 (2,4)=0 (2,5)=0 (2,6)=0 (2,7)=1 (2,8)=0 (2,9)=0
# Row 3: (3,4)=1 (3,5)=0 (3,6)=0 (3,7)=0 (3,8)=1 (3,9)=0
# Row 4: (4,5)=0 (4,6)=0 (4,7)=0 (4,8)=0 (4,9)=1
# Row 5: (5,6)=0 (5,7)=1 (5,8)=1 (5,9)=0
# Row 6: (6,7)=0 (6,8)=1 (6,9)=1
# Row 7: (7,8)=0 (7,9)=1
# Row 8: (8,9)=0
# Concat: 1 0011 0000 | 1 0001 000 | 1 00010 00 | 100010 0 | 00001 | 0110 | 011 | 01 | 0
# = 10011 00001 00010 001000 10001 00100 00001 01100 11010 000000
# Let me recalculate byte by byte:
# bit 0-7:   1,0,0,1,1,0,0,0 = 0x98
# bit 8-15:  0,1,0,0,0,1,0,0 = 0x44
# bit 16-23: 0,1,0,0,0,1,0,0 = 0x44
# bit 24-31: 1,0,0,0,1,0,0,0 = 0x88
# bit 32-39: 0,0,1,0,1,1,0,0 = 0x2C
# bit 40-44: 1,1,0,1,0        = 0xD0 (padded with 000)
# base64 of [0x98,0x44,0x44,0x88,0x2C,0xD0] = mEREiCzQ
post submit \
  '{"challenge_id":"ramsey:3:4:v1","graph":{"n":10,"encoding":"utri_b64_v1","bits_b64":"mEREiCzQ"}}' \
  "Petersen graph → R(3,4) [expect: accepted, n=10]"

# K5 (complete on 5 vertices) for R(3,3) — should be REJECTED (has triangle)
# All 10 upper-tri bits = 1 → 11111111 11 → 0xFF 0xC0 → base64: /8A=
post submit \
  '{"challenge_id":"ramsey:3:3:v1","graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"/8A="}}' \
  "K5 → R(3,3) [expect: rejected, clique_found]"

# Empty graph on 5 vertices — should be REJECTED (has independent set)
# All bits = 0 → 0x00 0x00 → base64: AAA=
post submit \
  '{"challenge_id":"ramsey:3:3:v1","graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"AAA="}}' \
  "E5 → R(3,3) [expect: rejected, independent_set_found]"

echo "=== Seeding complete ==="
echo ""
echo "Open http://localhost:5173 to explore the web app."
echo "Challenges with records should be visible at /challenges."
