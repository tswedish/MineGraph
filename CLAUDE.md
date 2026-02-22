# RamseyNet

Permissionless protocol for distributed Ramsey graph search and deterministic generative graph art.

## Build & Test (WSL2)

All commands run in WSL2 Ubuntu 24.04. Helper script: `scripts/wsl-dev.sh`.

```bash
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh ci       # Full CI: clippy + tests + web build
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh test     # cargo test --all
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh clippy   # cargo clippy
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh web      # pnpm install && pnpm build
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh sync     # rsync Windows → WSL2
wsl.exe -d Ubuntu -e bash scripts/wsl-dev.sh server   # API server on :3001
```

## Architecture

Rust workspace (`crates/`) + SvelteKit 2 (`web/`).

**Crate order:** `types` → `graph` → `verifier` → `ledger` → `server` ← `search`

| Crate | Purpose |
|-------|---------|
| `ramseynet-types` | Shared newtypes: GraphCid, ChallengeId, RamseyParams, Verdict |
| `ramseynet-graph` | RGXF encode/decode, AdjacencyMatrix, SHA-256 CID |
| `ramseynet-verifier` | Clique/independent-set detection, OVWC-1 WASM binary |
| `ramseynet-ledger` | SQLite ledger: challenges, submissions, receipts, records, events |
| `ramseynet-server` | Axum REST + WebSocket, full submit lifecycle |
| `ramseynet-search` | Greedy, local search, simulated annealing, worker loop |

## Server API

Port 3001, prefix `/api/`. SQLite at `./ramseynet.db`.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/challenges` | GET/POST | List or create challenges |
| `/api/challenges/{id}` | GET | Challenge detail + current record |
| `/api/records` | GET | Best-known records |
| `/api/verify` | POST | Stateless graph verification |
| `/api/submit` | POST | Full lifecycle: verify + store + record update |
| `/api/events` | WS | OESP-1 event stream |

## Key Specs

- **RGXF**: Packed upper-triangular adjacency bitstring, SHA-256 content addressed
- **OVWC-1**: Verifier contract — JSON stdin/stdout, exit 0
- **OESP-1**: WebSocket event stream with monotonic sequence numbers
