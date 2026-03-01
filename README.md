# RamseyNet

A permissionless protocol for distributed Ramsey graph search and deterministic generative graph art.

RamseyNet is a peer-to-peer network where anyone can propose and verify Ramsey graphs, persist artifacts in content-addressed storage, and derive public leaderboards and Pareto frontiers without central control.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 20+ with [pnpm](https://pnpm.io/)

### Build & Run

```
# Full CI: clippy + tests + web build
./run ci

# Start the API server (with logging)
./run server-log

# In another terminal — start the web dev server
./run web-dev

# In a third terminal — seed test data
./run seed
```

The server runs on `http://localhost:3001` and the web app on `http://localhost:5173`.

See **[TESTING.md](TESTING.md)** for a full interactive walkthrough.

### All Commands

```
./run ci          # Full CI: clippy + tests + web build
./run test        # Rust tests only
./run clippy      # Lint
./run build       # Build all crates
./run web         # Production web build
./run web-dev     # Web dev server (:5173)
./run server      # API server (:3001)
./run server-log  # API server with file logging
./run search      # Search worker (--k, --ell, --n required)
./run seed        # Seed test data
```

## Project Structure

```
crates/
  ramseynet-types/      Shared protocol types (GraphCid, RamseyParams, Verdict)
  ramseynet-graph/      RGXF graph encoding + SHA-256 content addressing
  ramseynet-verifier/   Ramsey verifier (clique detection, scoring, automorphism)
  ramseynet-ledger/     SQLite ledger (submissions, leaderboards, events)
  ramseynet-server/     Axum HTTP/WebSocket server
  ramseynet-search/     Graph search heuristics (greedy, local, SA, tree)
web/                    SvelteKit 2 / Svelte 5 frontend
test-vectors/           Shared test data (small_graphs.json)
scripts/                Dev helpers
```

## Web Application

The SvelteKit frontend provides:

- **Homepage** — Server health badge, navigation cards, live event feed
- **Leaderboards** (`/leaderboards`) — Browse by (K,L) pair, drill into n values
- **Leaderboard Detail** (`/leaderboards/[k]/[l]/[n]`) — Ranked table with scoring columns, top graph visualization
- **Submission Detail** (`/submissions/[cid]`) — Full graph details: verdict, witness, rank, matrix + circle visualization
- **Submit** (`/submit`) — Enter K/L/N, paste RGXF JSON, see live matrix preview, submit for verification
- **Live Events** — Real-time OESP-1 WebSocket event stream with auto-reconnect

### Graph Visualization

- **MatrixView** — Canvas-rendered adjacency matrix with witness overlay (red highlights for violating cliques/independent sets)
- **CircleLayout** — SVG circle graph with deterministic vertex placement (ORS-1.0)

## Server API

Port 3001, prefix `/api/`. SQLite at `./ramseynet.db`.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/leaderboards` | GET | List all (K,L,n) leaderboards with summary |
| `/api/leaderboards/{k}/{l}` | GET | List n values for a (K,L) pair |
| `/api/leaderboards/{k}/{l}/{n}` | GET | Full leaderboard with entries + top graph |
| `/api/leaderboards/{k}/{l}/{n}/threshold` | GET | Admission threshold (score-to-beat) |
| `/api/submissions/{cid}` | GET | Submission detail: graph, receipt, rank |
| `/api/verify` | POST | Stateless graph verification |
| `/api/submit` | POST | Full lifecycle: verify + store + leaderboard admit |
| `/api/events` | WS | OESP-1 event stream |

## Key Specs

- **RGXF**: Packed upper-triangular adjacency bitstring, SHA-256 content addressed
- **OVWC-1**: Verifier contract — JSON stdin/stdout, exit 0
- **OESP-1**: WebSocket event stream with monotonic sequence numbers

## Phase Status

| Phase | Status | Description |
|-------|--------|-------------|
| 0 — Scaffolding | Complete | Workspace, SvelteKit skeleton, CI |
| 1 — Graph Library | Complete | RGXF, AdjacencyMatrix, CID |
| 2 — Verifier | Complete | Clique detection, OVWC-1 WASM |
| 3 — Server + Ledger | Complete | Axum API, SQLite, WebSocket events |
| 4 — Web Application | Complete | Full interactive frontend |
| 5 — Search Worker | Complete | Greedy, local, SA, tree search |
| 5.5 — Leaderboard | Complete | (K,L,n)-indexed scoring, 100-cap leaderboards |
| 6 — P2P Networking | Pending | ed25519 identity, libp2p, duels |

## License

MIT
