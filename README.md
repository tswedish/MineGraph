# RamseyNet

A permissionless protocol for distributed Ramsey graph search and deterministic generative graph art.

RamseyNet is a peer-to-peer network where anyone can propose and verify Ramsey graphs, persist artifacts in content-addressed storage, and derive public leaderboards and Pareto frontiers without central control.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable, with `wasm32-wasip1` target)
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/) 9+
- WSL2 Ubuntu 24.04 (recommended dev environment)

### Build & Run

```bash
# Full CI: clippy + tests + web build
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh ci

# Start the API server (with logging)
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh server-log

# In another terminal — start the web app
wsl.exe -d Ubuntu -e bash -c "cd /root/RamseyNet/web && pnpm dev"

# In a third terminal — seed test data
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh seed
```

The server runs on `http://localhost:3001` and the web app on `http://localhost:5173`.

See **[TESTING.md](TESTING.md)** for a full interactive walkthrough.

## Project Structure

```
crates/
  ramseynet-types/      Shared protocol types (GraphCid, ChallengeId, Verdict)
  ramseynet-graph/      RGXF graph encoding + SHA-256 content addressing
  ramseynet-verifier/   Ramsey verifier (clique/independent-set detection)
  ramseynet-ledger/     SQLite ledger (challenges, submissions, records, events)
  ramseynet-server/     Axum HTTP/WebSocket server
  ramseynet-search/     Graph search heuristics (greedy, local search, SA)
web/                    SvelteKit 2 / Svelte 5 frontend
  src/lib/components/   MatrixView, CircleLayout, EventFeed, SubmitForm
  src/routes/           Homepage, /challenges, /challenges/[id], /submit
test-vectors/           Shared test data (small_graphs.json, verify_requests.json)
scripts/                Dev helpers (wsl-dev.sh, seed-ledger.sh)
docs/                   Whitepaper and specs
```

## Web Application

The SvelteKit frontend provides:

- **Homepage** — Server health badge, navigation cards, live event feed
- **Challenges** (`/challenges`) — Browse active Ramsey challenges with best-known records
- **Challenge Detail** (`/challenges/[id]`) — Record stats, adjacency matrix + circle graph visualization, inline submit form
- **Submit** (`/submit`) — Paste RGXF JSON, see live matrix preview, submit for verification
- **Live Events** — Real-time OESP-1 WebSocket event stream with auto-reconnect

### Graph Visualization

- **MatrixView** — Canvas-rendered adjacency matrix with witness overlay (red highlights for violating cliques/independent sets)
- **CircleLayout** — SVG circle graph with deterministic vertex placement (ORS-1.0)

## Server API

Port 3001, prefix `/api/`. SQLite at `./ramseynet.db`.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/challenges` | GET/POST | List or create challenges |
| `/api/challenges/{id}` | GET | Challenge detail + current record + record graph |
| `/api/records` | GET | Best-known records |
| `/api/verify` | POST | Stateless graph verification |
| `/api/submit` | POST | Full lifecycle: verify + store + record update |
| `/api/events` | WS | OESP-1 event stream |

## Key Specs

- **RGXF**: Packed upper-triangular adjacency bitstring, SHA-256 content addressed
- **OVWC-1**: Verifier contract — JSON stdin/stdout, exit 0
- **OESP-1**: WebSocket event stream with monotonic sequence numbers

## Development

```bash
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh ci         # Full CI
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh test       # Tests only
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh clippy     # Lint
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh web        # Web build
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh sync       # Windows → WSL2
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh server     # API server
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh server-log # API server with file logging
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh seed       # Seed test data
```

## Phase Status

| Phase | Status | Description |
|-------|--------|-------------|
| 0 — Scaffolding | Complete | Workspace, SvelteKit skeleton, CI |
| 1 — Graph Library | Complete | RGXF, AdjacencyMatrix, CID |
| 2 — Verifier | Complete | Clique detection, OVWC-1 WASM |
| 3 — Server + Ledger | Complete | Axum API, SQLite, WebSocket events |
| 4 — Web Application | Complete | Full interactive frontend |
| 5 — Search + Duels | Pending | Workers, ed25519 signing, duel system |
| 6 — P2P Networking | Pending | libp2p, multi-node replication |

## License

MIT
