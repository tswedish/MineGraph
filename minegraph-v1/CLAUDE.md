# MineGraph v1

Combinatorial graph search game with competitive leaderboards. Clean rewrite
of the RamseyNet prototype at `~/RamseyNet-dev/`.

## Quick Start

```bash
# CI
./run ci          # Full CI: fmt + clippy + tests
./run test        # Rust tests only

# Database setup (local Postgres on port 5432)
sudo -u postgres createuser minegraph
sudo -u postgres createdb -O minegraph minegraph
sudo -u postgres psql -c "ALTER USER minegraph WITH PASSWORD 'minegraph';"

# Server
cargo run -p minegraph-server -- --migrate \
  --database-url 'postgres://minegraph:minegraph@localhost/minegraph'

# CLI
cargo run -p minegraph-cli -- keygen --name "test"
cargo run -p minegraph-cli -- register-key
cargo run -p minegraph-cli -- submit --n 5 --graph6 'Dhc'
cargo run -p minegraph-cli -- leaderboard --n 5
cargo run -p minegraph-cli -- score --n 5 --graph6 'D~{'
cargo run -p minegraph-cli -- health
```

Other commands: `./run clippy`, `./run fmt`, `./run server`, `./run worker`.

## Architecture

Rust workspace (`crates/`) with 11 crates. Key differences from prototype:
- **graph6** format (not RGXF)
- **blake3** hashing (not SHA-256)
- **PostgreSQL** via sqlx (not SQLite)
- **Full k-clique histogram** scoring (not 4-tier)
- **Signatures required** (no anonymous submissions)
- **Shared identity crate** (no signing code duplication)
- **Server is API-only** (web apps are separate)
- **Leaderboards indexed by n only** (not k,ell,n)
- **SSE for real-time updates** (not WebSocket)

## Crate Dependency Graph

```
minegraph-types                    (leaf — no internal deps)
    |
    +-> minegraph-graph            (types)
    |       |
    |       +-> minegraph-scoring  (types, graph)
    |       |
    |       +-> minegraph-identity (types)
    |
    +-> minegraph-store            (types, graph, scoring, identity)
    +-> minegraph-server           (types, graph, scoring, identity, store)
    +-> minegraph-worker-api       (types, graph)
    +-> minegraph-worker-core      (types, graph, scoring, identity, worker-api)
    +-> minegraph-strategies       (types, graph, scoring, worker-api)
    +-> minegraph-worker           (worker-api, worker-core, strategies, identity)
    +-> minegraph-cli              (types, graph, scoring, identity)
```

## Current Status

Server, CLI, store, and scoring are fully implemented. Search worker is next.

### Implemented
- `minegraph-types` — GraphCid (blake3), KeyId, Verdict
- `minegraph-graph` — AdjacencyMatrix, graph6 encode/decode, blake3 CID
- `minegraph-scoring` — NeighborSet, bitwise clique counting, CliqueHistogram, Goodman (cross-validated), GraphScore with Ord, violation delta, guilty edges, fast fingerprint
- `minegraph-identity` — Ed25519 keypair generation, signing, verification, key file I/O (single source of truth for server + worker + CLI)
- `minegraph-store` — PostgreSQL models, migrations, 20+ repository methods, transactional leaderboard admission with rank recomputation
- `minegraph-server` — Full Axum API: health, leaderboards (paginated), submit (sig verify + score + admit + signed receipt), verify (stateless), identity CRUD, SSE event stream, proper error types
- `minegraph-worker-api` — SearchStrategy trait, SearchJob/Result, SearchObserver, WorkerCommand/Event/Status, ConfigParam
- `minegraph-cli` — init, keygen, whoami, register-key, score (local), submit, leaderboard, health

### TODO
1. `minegraph-strategies` — Port tree2 incremental beam search from prototype
2. `minegraph-worker-core` — Engine loop, server client, leaderboard sync, init modes
3. `minegraph-worker` — Worker CLI binary wiring
4. Canonical labeling via nauty (currently scores without canonical form)
5. Web apps (SvelteKit, leaderboard + dashboard)
6. CI workflow (GitHub Actions)

## Key Design Decisions

| Decision | Choice |
|----------|--------|
| Graph format | graph6 (standard, well-known) |
| Hashing | blake3 |
| Database | PostgreSQL (sqlx, runtime queries) |
| Signatures | Required (Ed25519, no anonymous) |
| Scoring | Full k-clique histogram, lexicographic |
| Canonical labeling | nauty (C FFI, same as prototype) — not yet wired |
| Real-time updates | Server-Sent Events (SSE) |
| Web UI | Separate SvelteKit apps (leaderboard + dashboard) |
| Worker plugins | Trait-based, statically linked |

## Scoring System

Golf-style (lower is better), lexicographic comparison:
1. For each k from max_k down to 3: `(max(red_k, blue_k), min(red_k, blue_k))`
2. Goodman gap (distance from theoretical minimum 3-clique count)
3. `1/|Aut(G)|` (more symmetric = lower = better)
4. CID bytes (deterministic tiebreaker)

## Server API

Port 3001, prefix `/api/`. All endpoints return JSON.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check + server identity |
| `/api/submit` | POST | Full lifecycle: verify sig, score, store, admit, sign receipt |
| `/api/verify` | POST | Stateless scoring (no DB write, no sig required) |
| `/api/leaderboards` | GET | List all n values with counts |
| `/api/leaderboards/{n}` | GET | Paginated leaderboard (`?limit=&offset=`) |
| `/api/leaderboards/{n}/threshold` | GET | Admission score threshold |
| `/api/leaderboards/{n}/cids` | GET | Incremental CID sync (`?since=`) |
| `/api/leaderboards/{n}/graphs` | GET | Batch graph6 download |
| `/api/submissions/{cid}` | GET | Submission detail + receipt + score |
| `/api/keys` | POST | Register public key |
| `/api/keys/{key_id}` | GET | Identity info |
| `/api/keys/{key_id}/submissions` | GET | Submissions by identity |
| `/api/events` | GET | SSE stream (admission + submission events) |

## Database

PostgreSQL with sqlx migrations in `migrations/`. Tables:
- `identities` — registered Ed25519 public keys
- `graphs` — deduplicated graphs (CID + graph6)
- `submissions` — signed submissions
- `scores` — precomputed histogram scores
- `leaderboard` — ranked entries per n
- `receipts` — server-signed verification results
- `server_config` — server-level configuration

### Local development database

```bash
sudo -u postgres createuser minegraph
sudo -u postgres createdb -O minegraph minegraph
sudo -u postgres psql -c "ALTER USER minegraph WITH PASSWORD 'minegraph';"
```

Connection URL: `postgres://minegraph:minegraph@localhost/minegraph`

### Docker (alternative, uses port 5433)

```bash
docker-compose up -d
# URL: postgres://minegraph:minegraph@localhost:5433/minegraph
```

## Prototype Reference

The RamseyNet prototype at `~/RamseyNet-dev/` has proven implementations of:
- Bitwise incremental beam search (tree2) — to be ported
- Evolutionary simulated annealing (evo) — to be ported
- Fleet/experiment infrastructure
- GemView rendering
- SvelteKit leaderboard web app
- nauty canonical labeling + automorphism — to be ported

See `~/RamseyNet-dev/CLAUDE.md` for prototype details.

## Testing

54 tests across the foundation crates. Run with `cargo test`.
Clippy clean (`-D warnings`), `cargo fmt` clean.
