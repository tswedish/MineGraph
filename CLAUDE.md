# MineGraph v1

Combinatorial graph search game with competitive leaderboards.

## Quick Start

```bash
# CI
./run ci          # Full CI: fmt + clippy + tests
./run test        # Rust tests only

# Database setup (local Postgres on port 5432)
sudo -u postgres createuser minegraph
sudo -u postgres createdb -O minegraph minegraph
sudo -u postgres psql -c "ALTER USER minegraph WITH PASSWORD 'minegraph';"

# Environment (reads DATABASE_URL from env or .env)
cp .env.example .env

# Server (first run with --migrate, then just cargo run)
cargo run -p minegraph-server -- --migrate

# Dashboard relay server (for worker monitoring)
./run dashboard

# Dashboard web UI (SvelteKit, port 5174)
./run dashboard-ui

# Server web app (SvelteKit, port 5173)
./run web-dev

# Worker (with dashboard connection)
cargo run -p minegraph-worker -- --n 25 --beam-width 80 --max-depth 12 --dashboard ws://localhost:4000/ws/worker

# Experiment fleet (8 diverse workers, release build)
./scripts/experiment.sh 25

# Fleet (uniform workers)
./scripts/fleet.sh --workers 4 --n 25 --release --dashboard ws://localhost:4000/ws/worker

# CLI
cargo run -p minegraph-cli -- keygen --name "test"
cargo run -p minegraph-cli -- register-key
cargo run -p minegraph-cli -- submit --n 5 --graph6 'Dhc'
cargo run -p minegraph-cli -- leaderboard --n 25
cargo run -p minegraph-cli -- score --n 5 --graph6 'D~{'
cargo run -p minegraph-cli -- health
```

Other commands: `./run clippy`, `./run fmt`, `./run server`, `./run worker`, `./run dashboard`, `./run dashboard-ui`, `./run web-dev`, `./run web-build`.

Logging: `RUST_LOG=debug cargo run -p minegraph-server` (default: info).

## Architecture

Rust workspace (`crates/`) with 12 crates + 2 SvelteKit web apps + shared component package:

- **graph6** format for graph encoding
- **blake3** hashing for CIDs
- **PostgreSQL** via sqlx
- **Full k-clique histogram** scoring (lexicographic)
- **Ed25519 signatures required** (no anonymous submissions)
- **Server is API-only** (web apps are separate)
- **Leaderboards indexed by n only**
- **SSE for real-time updates** (enriched with graph6/scores for visualization)
- **Paley graph fallback** for cold-start seeding
- **Dashboard relay server** for worker monitoring (separate from leaderboard server)
- **WebSocket telemetry** from workers to dashboard relay

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
    +-> minegraph-worker-core      (types, graph, scoring, identity, worker-api, strategies, dashboard)
    +-> minegraph-strategies       (types, graph, scoring, worker-api)
    +-> minegraph-worker           (worker-api, worker-core, strategies, identity)
    +-> minegraph-cli              (types, graph, scoring, identity)
    +-> minegraph-dashboard        (identity) — standalone dashboard relay server
```

## Web Apps & Frontend

```
web/                     Server web app (SvelteKit, port 5173)
dashboard/               Worker dashboard app (SvelteKit, port 5174)
packages/shared/         Shared components (@minegraph/shared)
  src/components/
    GemView.svelte         Diamond-rotated adjacency matrix visualization
    GemViewSquare.svelte   Rain column variant (opacity, glow, click props)
    GemPopup.svelte        Click-to-expand detail modal
  src/graph6.ts            graph6 decoder
  src/types.ts             Shared TypeScript types
```

npm workspaces: root `package.json` manages `packages/shared`, `web`, `dashboard`.

## Current Status

All 12 backend crates implemented and working end-to-end. 82 tests passing.

### Implemented
- `minegraph-types` — GraphCid (blake3), KeyId, Verdict
- `minegraph-graph` — AdjacencyMatrix, graph6 encode/decode, blake3 CID
- `minegraph-scoring` — NeighborSet, bitwise clique counting, CliqueHistogram, Goodman (cross-validated), GraphScore with Ord, violation delta, guilty edges, fast fingerprint, canonical labeling via nauty
- `minegraph-identity` — Ed25519 keypair, signing, verification, key file I/O (single source of truth)
- `minegraph-store` — PostgreSQL models, 2 migrations, 20+ repository methods, lightweight leaderboard admission (no full-table rerank)
- `minegraph-server` — Axum API (11 endpoints): health, leaderboards, submit, verify, identity, SSE events (enriched with graph6/scores), signed receipts. Structured logging for submissions/admissions/connections.
- `minegraph-worker-api` — SearchStrategy trait, SearchJob/Result, SearchObserver (CollectingObserver), WorkerCommand/Event/Status, ConfigParam
- `minegraph-strategies` — tree2 beam search (passes R(3,3)/n=5 and R(4,4)/n=17 tests), Paley graph init, perturb
- `minegraph-worker-core` — Engine loop with server client, leaderboard CID sync, biased seed sampling, Paley fallback for cold start, DashboardObserver for real-time telemetry, priority-sorted submit buffer (best graphs submitted first), throttled progress events (4 Hz)
- `minegraph-worker` — Full CLI binary: n, target_k, target_ell, beam_width, max_depth, sample_bias, focused, offline, signing key, metadata, dashboard URL
- `minegraph-cli` — init, keygen (with --output), whoami, register-key, score (local), submit, leaderboard, health
- `minegraph-dashboard` — Standalone Axum relay server: worker WebSocket endpoint, browser WebSocket endpoint (multiplexed), REST API for worker listing, key_id allow-list auth (default open), static file serving
- **Server web app** (`web/`) — SvelteKit: home, leaderboards (paginated with GemView), activity dashboard (submission-inferred), rain visualization (SSE-driven), submission detail, identity profiles
- **Worker dashboard** (`dashboard/`) — SvelteKit: monitor mode (live worker stats, progress bars, gem thumbnails), rain mode (vertical gem columns per worker, current search at top, best-found pool below), controls (gem size, fade duration 10m-8h, history depth 10-200), fullscreen mode
- **Shared components** (`packages/shared/`) — GemView (diamond adjacency matrix), GemViewSquare (rain variant), GemPopup (detail modal), graph6 decoder

### TODO
1. Server rain rewrite — column-based layout driven by SSE submission events (Phase 6 of dashboard plan)
2. Evo strategy port
3. Production hardening (rate limiting, connection pool tuning)
4. Dashboard auth — Ed25519 challenge/response for worker registration (currently just key_id check)
5. Column drag reorder in rain mode
6. Worker command/control via dashboard (pause/resume/reconfigure)

## Key Design Decisions

| Decision | Choice |
|----------|--------|
| Graph format | graph6 (standard, well-known) |
| Hashing | blake3 |
| Database | PostgreSQL (sqlx, runtime queries) |
| Signatures | Required (Ed25519, no anonymous) |
| Scoring | Full k-clique histogram, lexicographic, golf-style |
| Canonical labeling | nauty (C FFI) — wired into scoring and worker |
| Real-time updates | SSE (server), WebSocket (dashboard relay) |
| Web UI | Separate SvelteKit apps (server + dashboard) |
| Worker monitoring | Dedicated dashboard relay server (not on leaderboard server) |
| Worker plugins | Trait-based, statically linked |
| Leaderboard admission | Lightweight: count-based rank, no full rerank |
| Submit buffer | Priority-sorted (best score first) |

## Scoring System

Golf-style (lower is better), lexicographic comparison:
1. For each k from max_k down to 3: `(max(red_k, blue_k), min(red_k, blue_k))`
2. Goodman gap (distance from theoretical minimum 3-clique count)
3. `1/|Aut(G)|` (more symmetric = lower = better)
4. CID bytes (deterministic tiebreaker)

Worker passes Ramsey target via strategy config: `target_k` and `target_ell`
(default: 5, 5 for R(5,5) search). Leaderboard is indexed by n only.

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
| `/api/keys/{key_id}` | GET | Identity info (with leaderboard_limit param) |
| `/api/keys/{key_id}/submissions` | GET | Submissions by identity |
| `/api/events` | GET | SSE stream (enriched admission + submission events) |

## Dashboard Relay Server

Port 4000. Separate from the leaderboard server.

| Endpoint | Type | Description |
|----------|------|-------------|
| `/ws/worker` | WebSocket | Workers register and stream telemetry |
| `/ws/ui` | WebSocket | Browser receives multiplexed worker events |
| `/api/workers` | GET | List connected workers |
| `/api/config` | GET | Dashboard configuration |

**Protocol**: Workers send `Register`, `Progress`, `Discovery`, `RoundComplete` messages. Dashboard relays to browser as `WorkerConnected`, `WorkerDisconnected`, `WorkerEvent` envelopes.

## Worker Configuration

```
--n 25                    Target vertex count
--target-k 5              Clique size in graph (default 5)
--target-ell 5            Clique size in complement (default 5)
--beam-width 80           Beam candidates per depth
--max-depth 12            Search depth levels
--max-iters 100000        Iteration budget per round
--sample-bias 0.8         Leaderboard seed bias (0=uniform, 1=top)
--focused false           Focused edge flipping
--noise-flips 0           Random flips on seed
--offline                 Local-only (no server)
--signing-key PATH        Ed25519 key (or auto-detect .config/minegraph/key.json)
--dashboard URL            Dashboard relay WebSocket URL
--metadata JSON            Metadata JSON (max 4KB, attached to submissions)
```

### Tuning guide (from experiments on n=25)

Best performers use **moderate noise (1-3 flips)** and **low sample bias (0.3-0.6)**:
- Wide beam (150-200) + shallow depth (8-10): most discoveries, broad exploration
- Focused mode + noise (2 flips): high admission rate, surgical improvements
- Deep beam (40-60) + high depth (16-20): fewer but higher-quality discoveries
- High noise (>5 flips): too destructive, poor results

## Database

PostgreSQL with sqlx migrations in `migrations/`. Leaderboard PK is `(n, cid)`.
Rank is computed at insertion time, queries sort by `score_bytes` directly.

Tables: `identities`, `graphs`, `submissions`, `scores`, `leaderboard`,
`receipts`, `server_config`.

### Local setup

```bash
sudo -u postgres createuser minegraph
sudo -u postgres createdb -O minegraph minegraph
sudo -u postgres psql -c "ALTER USER minegraph WITH PASSWORD 'minegraph';"
```

### Persistent server key

```bash
cargo run -p minegraph-cli -- keygen --name "my-server" -o .config/minegraph/server-key.json
export SERVER_KEY_PATH=.config/minegraph/server-key.json
cargo run -p minegraph-server -- --migrate
```

## Testing

82 tests across all crates. Run with `cargo test`.
Clippy clean (`-D warnings`), `cargo fmt` clean.
CI: `.github/workflows/ci.yml` (fmt + clippy + test).

## Performance Notes

Worker→dashboard telemetry is throttled at multiple levels:
- Worker: Progress events at 4 Hz max, Discovery events capped at 20/round
- Worker→relay channel: bounded (64 msgs), `try_send` drops excess
- Relay broadcast: 256 capacity, lagged receivers skip
- Browser: messages batched per 500ms flush, Progress deduped per worker
- Submit buffer: priority-sorted by score (best first)
