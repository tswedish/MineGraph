# RamseyNet — Human Testing Guide

Interactive walkthrough for testing the RamseyNet web application and API. Follow these steps to set up a local test environment, explore every feature, and capture logs for review.

---

## 1. Prerequisites

- **WSL2 Ubuntu 24.04** with Rust stable + pnpm/Node.js (see CLAUDE.md)
- **curl** and optionally **jq** (for pretty JSON output)
- A modern web browser (Chrome/Firefox/Edge)

---

## 2. Setting Up the Test Environment

All commands below run **inside the WSL2 shell** (e.g., a tmux session or Windows Terminal WSL tab) from the repo root.

> **From Windows PowerShell?** Prefix any command with `wsl.exe -d Ubuntu -e`, e.g.:
> `wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh ci`

### 2a. Build and verify tests pass

```bash
bash scripts/wsl-dev.sh ci
```

You should see `=== CI passed! ===` at the end. This confirms all Rust tests pass, clippy is clean, and the web app builds.

### 2b. Start the API server with logging

Open a **dedicated terminal** (or tmux pane) for the server. Logs will be saved to `logs/server-<timestamp>.log`:

```bash
bash scripts/wsl-dev.sh server-log
```

The server starts on **http://localhost:3001**. Keep this running — all API requests will be logged here and to the log file.

### 2c. Start the web dev server

Open a **second terminal** (or tmux pane):

```bash
bash scripts/wsl-dev.sh web-dev
```

The web app starts on **http://localhost:5173**.

### 2d. Seed the database with test data

Open a **third terminal** (or tmux pane) and run the seed script:

```bash
bash scripts/wsl-dev.sh seed
```

This creates three challenges (R(3,3), R(3,4), R(4,4)) and submits four graphs:
- **C5** → R(3,3) — accepted, sets record at n=5
- **Petersen graph** → R(3,4) — accepted, sets record at n=10
- **K5** → R(3,3) — rejected (clique_found, witness [0,1,2])
- **E5 (empty)** → R(3,3) — rejected (independent_set_found, witness [0,1,2])

You should see `[201]` or `[200]` status codes for each operation.

### 2e. Retrieving logs after testing

Server logs are saved in `logs/` under the repo root. To view:

```bash
# List log files
ls -la logs/

# View the latest log
cat logs/$(ls -t logs/ | head -1)
```

Browser console logs can be captured via DevTools (F12 → Console tab) or by opening `chrome://net-export/` in Chrome for network-level logs.

---

## 3. Feature Walkthrough

Open **http://localhost:5173** in your browser and follow these steps.

### 3.1 Homepage

**What to verify:**
- [ ] The RamseyNet title renders with a purple gradient
- [ ] The green status badge shows `RamseyNet v0.1.0 — ok`
- [ ] Three cards appear: Challenges, Submit, Records
- [ ] The **Live Events** panel at the bottom shows a green connection dot
- [ ] Events from the seed script appear (challenge.created, graph.submitted, graph.verified, record.updated)
- [ ] Events are color-coded: blue for created, green for verified, amber for record.updated

### 3.2 Challenges List (`/challenges`)

Click **"View challenges"** or the **Challenges** nav link.

**What to verify:**
- [ ] Three challenge cards appear: R(3,3), R(3,4), R(4,4)
- [ ] R(3,3) shows `n = 5` in green (the C5 record)
- [ ] R(3,4) shows `n = 10` in green (the Petersen graph record)
- [ ] R(4,4) shows `no submissions` (no record yet)
- [ ] Each card shows its challenge_id and description
- [ ] Cards show truncated CIDs for challenges with records
- [ ] Hovering a card highlights its border in purple

### 3.3 Challenge Detail (`/challenges/ramsey:3:3:v1`)

Click the **R(3,3)** card.

**What to verify:**
- [ ] Back link ("← Challenges") navigates back to the list
- [ ] Header shows `R(3,3)` with challenge_id `ramsey:3:3:v1`
- [ ] Description text appears
- [ ] **Current Record** section shows: Best n = 5, full CID, timestamp
- [ ] **Matrix View** (left): 5×5 adjacency matrix with the C5 pattern — a checkerboard-like cycle with diagonal empty cells
- [ ] **Circle Layout** (right): Pentagon with 5 vertices connected in a cycle
- [ ] Vertex labels (0–4) appear on both visualizations
- [ ] Scrolling down reveals the **Submit a Graph** form
- [ ] The challenge dropdown is pre-selected to `ramsey:3:3:v1`

### 3.4 Challenge Detail — R(3,4) with Petersen Graph

Navigate to `/challenges/ramsey:3:4:v1` (use the back link → click R(3,4) card).

**What to verify:**
- [ ] Best n = 10 with the Petersen graph CID
- [ ] Matrix View shows a 10×10 grid (the Petersen graph adjacency)
- [ ] Circle Layout shows 10 vertices in a circle with the 3-regular edge pattern
- [ ] The Petersen graph has a distinctive symmetric pattern in both views

### 3.5 Submit Page (`/submit`)

Click the **Submit** nav link.

**What to verify:**
- [ ] Page title "Submit a Graph" with subtitle
- [ ] Challenge dropdown lists all three challenges
- [ ] RGXF JSON textarea with placeholder text
- [ ] Submit button is disabled when no challenge or graph is entered

#### Test A: Submit an accepted graph

1. Select **ramsey:4:4:v1** from the dropdown
2. Paste this C5 graph into the textarea:
   ```json
   {"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}
   ```
3. **Verify:** A live matrix preview appears below the textarea showing the C5 adjacency pattern
4. **Verify:** The preview label shows "Preview (n=5)"
5. Click **Submit Graph**
6. **Verify:** A green-bordered result box appears with:
   - `ACCEPTED` in green
   - `New record!` in amber
   - The graph CID
7. **Verify:** The Submit button briefly shows "Submitting..." while processing

#### Test B: Submit a rejected graph

1. Keep **ramsey:4:4:v1** selected (or switch to ramsey:3:3:v1)
2. Clear the textarea and paste this K4 (complete on 4 vertices):
   ```json
   {"n": 4, "encoding": "utri_b64_v1", "bits_b64": "/A=="}
   ```
3. **Verify:** Preview shows a 4×4 fully-connected matrix
4. Click **Submit Graph**
5. **Verify:** A red-bordered result box appears with:
   - `REJECTED` in red
   - Reason: `clique_found`
   - Witness vertices: `[0, 1, 2, 3]` in red
   - A witness overlay matrix where the witness rows/columns are highlighted in red

#### Test C: Invalid JSON handling

1. Type `not valid json` in the textarea
2. **Verify:** An "Invalid JSON" error appears below the textarea in red
3. **Verify:** No preview is shown
4. **Verify:** The Submit button is disabled

#### Test D: Missing fields

1. Type `{"n": 5}` in the textarea
2. **Verify:** A "Missing required fields" error appears

### 3.6 Live Events (WebSocket)

Go back to the homepage (**RamseyNet** logo link).

**What to verify:**
- [ ] The Live Events panel shows all events from your submissions in Test A/B above
- [ ] New events appear at the top (newest first)
- [ ] Sequence numbers (#1, #2, ...) are monotonically increasing
- [ ] The green dot indicates an active WebSocket connection

**Disconnect test:**
1. Stop the API server (Ctrl+C in the server terminal)
2. **Verify:** The connection dot turns red
3. Restart the server: `bash scripts/wsl-dev.sh server-log`
4. **Verify:** The dot turns green again after a few seconds (auto-reconnect)

### 3.7 Navigation

**What to verify:**
- [ ] The **RamseyNet** logo always navigates to the homepage
- [ ] **Challenges** nav link goes to `/challenges`
- [ ] **Submit** nav link goes to `/submit`
- [ ] Browser back/forward buttons work correctly (SPA routing)
- [ ] Directly visiting a URL like `/challenges/ramsey:3:3:v1` loads the page (no 404)

---

## 4. API Testing with curl

You can also test the API directly. These commands assume the server is running on port 3001.

### Health check

```bash
curl -s http://localhost:3001/api/health | jq .
```

### List challenges

```bash
curl -s http://localhost:3001/api/challenges | jq .
```

### Get challenge detail (includes record_graph for visualization)

```bash
curl -s http://localhost:3001/api/challenges/ramsey:3:3:v1 | jq .
```

### List records

```bash
curl -s http://localhost:3001/api/records | jq .
```

### Stateless verification (no database)

```bash
curl -s -X POST http://localhost:3001/api/verify \
  -H "Content-Type: application/json" \
  -d '{
    "oras_version": "ovwc-1",
    "k": 3, "ell": 3,
    "graph": {"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="},
    "want_cid": true
  }' | jq .
```

### Submit a graph (full lifecycle)

```bash
curl -s -X POST http://localhost:3001/api/submit \
  -H "Content-Type: application/json" \
  -d '{
    "challenge_id": "ramsey:4:4:v1",
    "graph": {"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}
  }' | jq .
```

### WebSocket event stream

```bash
# Requires websocat or wscat
# Install: npm install -g wscat
wscat -c ws://localhost:3001/api/events
```

---

## 5. Test Graphs Reference

These graphs are available in `test-vectors/small_graphs.json`:

| Graph | n | RGXF bits_b64 | R(3,3) | R(3,4) | Notes |
|-------|---|---------------|--------|--------|-------|
| C5 (5-cycle) | 5 | `mUA=` | accepted | accepted | omega=2, alpha=2 |
| K5 (complete) | 5 | `/8A=` | rejected | rejected | omega=5, witness [0,1,2] |
| E5 (empty) | 5 | `AAA=` | rejected | rejected | alpha=5, witness [0,1,2] |
| Petersen | 10 | `mEREiCzQ` | accepted | accepted | omega=2, alpha=4, 3-regular |
| K4 (complete) | 4 | `/A==` | rejected | — | omega=4, witness [0,1,2] |

To submit any of these via the web UI, paste the RGXF JSON in this format:
```json
{"n": 5, "encoding": "utri_b64_v1", "bits_b64": "mUA="}
```

---

## 6. Known Limitations (Phase 4)

- No authentication or signing (Phase 5)
- No P2P networking (Phase 6)
- Database is local SQLite — restarting the server preserves data, but the DB file is per-environment
- The Petersen graph encoding in the seed script has not been independently verified for R(3,4); if it fails, the server will correctly report the rejection
- Browser console may show WebSocket close/reconnect messages when the server restarts — this is expected

---

## 7. Cleanup

To start fresh with a clean database:

```bash
rm -f ramseynet.db ramseynet.db-wal ramseynet.db-shm
```

Then restart the server and re-run the seed script.
