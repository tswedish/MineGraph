# MineGraph — Next Steps

Current state as of 2026-03-15.

## Where We Are

- **tree2 is the production baseline.** It's ~11x faster than tree per round,
  gets ~4.6x more leaderboard admissions, and is now the default strategy.
- **16-worker fleet** running tree2 against R(5,5) n=25, ~80% CPU utilization.
- **Leaderboard is saturating** — admission rates dropping, tree fully plateaued,
  tree2 approaching plateau with ~6K total admissions into 500-slot boards.
- **First experiment data collected** — tree vs tree2 head-to-head with analysis.

## Main Improvement Path (RESUME HERE)

### Priority 1: Bitwise Adjacency Operations (5-10x speedup)

**This is the single highest-leverage change available.**

The clique-checking inner loop currently uses scalar `edge()` calls (index math +
byte lookup per check). For n=25, the entire adjacency matrix is 300 bits = 5 `u64`
words. Rewriting the inner loop to use bitwise AND/OR/popcount operations would:

- Replace ~46 `edge()` calls per common-neighbor scan with 1 AND + 1 popcount
- Replace recursive backtracking with nested bit iteration
- Eliminate all `Vec` allocations in the hot path
- **Expected speedup: 5-10x** on the same hardware

This makes 16 workers equivalent to 80-160 workers with current code.

**Files to change:**
- `crates/ramseynet-graph/src/adjacency.rs` — add `neighbor_masks()` method
- `crates/ramseynet-strategies/src/incremental.rs` — add bitwise counting functions + `NeighborSet` type
- `crates/ramseynet-strategies/src/tree2.rs` — update `BeamEntry` to carry `NeighborSet`
- `crates/ramseynet-strategies/src/evo.rs` — update `Individual` to carry `NeighborSet`

**Design is fully specified** — see conversation history for the complete implementation plan
with data structures, algorithms, and performance estimates.

### Priority 2: Diversity-Aware Beam Selection

Once the leaderboard saturates, finding *more* valid graphs doesn't help — we need
*better-scoring* graphs or graphs in unexplored regions. Options:

- Add a novelty bonus to beam selection (penalize candidates similar to existing beam members)
- Maintain a fingerprint archive across rounds
- Use graph invariants (degree sequence, triangle count) as diversity signals

### Priority 3: Score-Aware Search

Current search optimizes violation count (reach 0 = valid). Once valid, all graphs
are treated equally. To improve leaderboard rank, the search should:

- Among valid candidates in the beam, prefer those with better Goodman gap
- Use automorphism-group-order proxy during search (expensive, but only for valid candidates)

### Priority 4: GPU Batch Evaluation (after bitwise)

Once the inner loop is bitwise (no branching, no recursion), it maps trivially to GPU:
- Each CUDA thread processes one (parent, edge) pair
- Zero warp divergence
- RTX 4070 available with 12GB VRAM, currently unused
- Expected additional 10-50x on top of bitwise CPU

## Experiment Infrastructure

- `./run experiment` — head-to-head strategy comparison
- `./run fleet` — launch N workers against same server
- `./scripts/analyze_experiment.sh` — compact analysis of experiment logs
- Logs in `logs/experiment-*/` and `logs/fleet-*/`

## Side Quests (fun but not on critical path)

- **MineGraph Gem renderer** — `minegraph_gem_v2.py`, deterministic pixel-art from graphs
- **Web integration** — render gems in the leaderboard web app
- **Public deployment** — Cloud Run server, public leaderboard, contributor workflows
