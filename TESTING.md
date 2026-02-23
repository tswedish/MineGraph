# RamseyNet — Testing Guide

Interactive walkthrough for testing the web application and API.

---

## 1. Prerequisites

- Rust stable + pnpm/Node.js (see README)
- **curl** and optionally **jq**
- A modern web browser

---

## 2. Setup

### 2a. Build and verify tests pass

```
./run ci
```

You should see `=== CI passed! ===` at the end.

### 2b. Start the API server

Open a terminal for the server (logs saved to `logs/`):

```
./run server-log
```

Server starts on **http://localhost:3001**.

### 2c. Start the web dev server

In a second terminal:

```
./run web-dev
```

Web app starts on **http://localhost:5173**.

### 2d. Seed test data

In a third terminal:

```
./run seed
```

This creates three challenges (R(3,3), R(3,4), R(4,4)) and submits four graphs:
- **C5** → R(3,3) — accepted, record n=5
- **Wagner graph** → R(3,4) — accepted, record n=8
- **K5** → R(3,3) — rejected (clique_found, witness [0,1,2])
- **E5 (empty)** → R(3,3) — rejected (independent_set_found, witness [0,1,2])

### 2e. Server logs

```
ls -la logs/
```

---

## 3. Feature Walkthrough

Open **http://localhost:5173** in your browser.

### 3.1 Homepage

- [ ] RamseyNet title renders with purple gradient
- [ ] Green status badge shows `RamseyNet v0.1.0 — ok`
- [ ] Three navigation cards: Challenges, Submit, Records
- [ ] Live Events panel shows a green connection dot
- [ ] Events from the seed script appear (challenge.created, graph.submitted, etc.)
- [ ] Events with CIDs are clickable links to submission detail pages
- [ ] No favicon 404 in the network tab (SVG K5 favicon loads)

### 3.2 Challenges List (`/challenges`)

Click **"View challenges"** or the **Challenges** nav link.

- [ ] Three challenge cards: R(3,3), R(3,4), R(4,4)
- [ ] R(3,3) shows `n = 5` in green
- [ ] R(3,4) shows `n = 8` in green
- [ ] R(4,4) shows `no submissions`

### 3.3 Challenge Detail (`/challenges/ramsey:3:3:v1`)

Click the **R(3,3)** card.

- [ ] Back link navigates to challenge list
- [ ] Header: `R(3,3)` with `ramsey:3:3:v1`
- [ ] Current Record: Best n = 5, full CID (clickable → submission detail), timestamp
- [ ] Matrix View: 5x5 adjacency matrix with C5 pattern
- [ ] Circle Layout: Pentagon with cycle edges
- [ ] Submit form pre-selected to `ramsey:3:3:v1`

### 3.4 Submission Detail (`/submissions/[cid]`)

Click the CID link on any challenge record, event feed entry, or records table row.

- [ ] Back button navigates to previous page
- [ ] Full CID displayed in monospace header
- [ ] Challenge link → navigates to `/challenges/[id]` with R(k,l) label
- [ ] Graph size (n) displayed
- [ ] Verdict badge: green ACCEPTED or red REJECTED
- [ ] Reason text (if rejected)
- [ ] Witness vertices (if present)
- [ ] Matrix View + Circle Layout side-by-side (with witness overlay for rejected graphs)
- [ ] Submitted and Verified timestamps

### 3.5 Records (`/records`)

Click the **Records** nav link.

- [ ] Table with Challenge, Best n, CID, Updated columns
- [ ] CID column entries are clickable links to submission detail
- [ ] Challenge column links to challenge detail

### 3.6 Submit Page (`/submit`)

Click the **Submit** nav link.

- [ ] Challenge dropdown lists all three challenges
- [ ] RGXF JSON textarea with placeholder

#### Test A: Accepted graph

1. Select **ramsey:4:4:v1**
2. Paste: `{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}`
3. Verify: Live matrix preview appears
4. Click **Submit Graph**
5. Verify: Green result with `ACCEPTED` and `New record!`

#### Test B: Rejected graph

1. Paste: `{"n": 4, "encoding": "utri_b64_v1", "bits_b64": "/A=="}`
2. Click **Submit Graph**
3. Verify: Red result with `REJECTED`, reason `clique_found`, witness `[0, 1, 2, 3]`

#### Test C: Invalid input

1. Type `not valid json` → "Invalid JSON" error, submit disabled
2. Type `{"n": 5}` → "Missing required fields" error

### 3.7 Navigation

- [ ] RamseyNet logo → homepage
- [ ] Challenges / Submit / Records nav links work
- [ ] Browser back/forward works (SPA routing)
- [ ] Direct URL access works (e.g., `/challenges/ramsey:3:3:v1`)

### 3.8 WebSocket Reconnect

1. Stop the API server (Ctrl+C)
2. Verify: Connection dot turns red
3. Restart: `./run server-log`
4. Verify: Dot turns green within a few seconds

---

## 4. API Testing with curl

```bash
# Health check
curl -s localhost:3001/api/health | jq .

# List challenges
curl -s localhost:3001/api/challenges | jq .

# Challenge detail
curl -s localhost:3001/api/challenges/ramsey:3:3:v1 | jq .

# Records
curl -s localhost:3001/api/records | jq .

# Submission detail (replace CID with a real one from records)
curl -s localhost:3001/api/submissions/<cid> | jq .

# Stateless verification
curl -s -X POST localhost:3001/api/verify \
  -H "Content-Type: application/json" \
  -d '{"oras_version":"ovwc-1","k":3,"ell":3,"graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"mUA="},"want_cid":true}' | jq .

# Submit a graph
curl -s -X POST localhost:3001/api/submit \
  -H "Content-Type: application/json" \
  -d '{"challenge_id":"ramsey:4:4:v1","graph":{"n":5,"encoding":"utri_b64_v1","bits_b64":"mUA="}}' | jq .

# WebSocket event stream (requires wscat: npm install -g wscat)
wscat -c ws://localhost:3001/api/events
```

---

## 5. Test Graphs Reference

From `test-vectors/small_graphs.json`:

| Graph | n | bits_b64 | R(3,3) | R(3,4) |
|-------|---|----------|--------|--------|
| C5 (5-cycle) | 5 | `mUA=` | accepted | accepted |
| K5 (complete) | 5 | `/8A=` | rejected | rejected |
| E5 (empty) | 5 | `AAA=` | rejected | rejected |
| Petersen | 10 | `mEREiCzQ` | — | rejected |
| Wagner | 8 | `kySmUA==` | — | accepted |
| K4 (complete) | 4 | `/A==` | rejected | — |

RGXF JSON format: `{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}`

---

## 6. Cleanup

To start fresh:

```
rm -f ramseynet.db ramseynet.db-wal ramseynet.db-shm
```

Then restart the server and re-seed.

---

## 7. Known Limitations

- No authentication or signing (Phase 5)
- No P2P networking (Phase 6)
- Database is local SQLite — persists across restarts but is per-environment
- WebSocket close/reconnect messages in browser console on server restart are expected
