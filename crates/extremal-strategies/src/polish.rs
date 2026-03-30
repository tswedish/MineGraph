//! Score-aware polishing for valid Ramsey graphs.
//!
//! When a strategy finds a valid graph (zero violations), polish explores
//! the valid-graph landscape via a tabu walk to find better-scoring variants.
//! Uses incremental clique-count deltas for efficiency: O(n^2) per candidate
//! edge for 4-clique deltas instead of O(n^4) full recount.
//!
//! The key insight: among valid R(k,l) graphs, the leaderboard score is
//! dominated by 4-clique counts, then triangle balance. The tabu walk
//! explores far more of the valid-graph neighborhood than a greedy
//! hill-climb (500+ steps vs 3 steps).
//!
//! ## 2-opt mode
//!
//! When enabled, the tabu walk also considers *paired* edge flips: two edges
//! flipped simultaneously where neither flip alone preserves validity but the
//! pair does. This expands the reachable neighborhood beyond single-flip
//! connectivity, allowing escape from quality basins that exhaust all
//! single-flip improvements. Activated only after the walk stalls (no
//! best-score improvement for 10 steps) to keep overhead low.

use std::collections::HashSet;

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques, violation_delta};
use extremal_types::GraphCid;
use extremal_worker_api::{RawDiscovery, SearchObserver};
use rand::Rng;
use rand::seq::SliceRandom;
use tracing::debug;

/// Score tuple for comparing valid graphs: (max_4c, min_4c, max_3c, min_3c).
/// Lower is better (golf-style, matching leaderboard ordering).
type ValidScore = (u64, u64, u64, u64);

/// Absolute clique counts for incremental score tracking.
struct ScoreState {
    red_4: u64,
    blue_4: u64,
    red_3: u64,
    blue_3: u64,
}

impl ScoreState {
    fn from_counts(adj_nbrs: &NeighborSet, comp_nbrs: &NeighborSet, n: u32) -> Self {
        Self {
            red_4: count_cliques(adj_nbrs, 4, n),
            blue_4: count_cliques(comp_nbrs, 4, n),
            red_3: count_cliques(adj_nbrs, 3, n),
            blue_3: count_cliques(comp_nbrs, 3, n),
        }
    }

    fn score_tuple(&self) -> ValidScore {
        (
            self.red_4.max(self.blue_4),
            self.red_4.min(self.blue_4),
            self.red_3.max(self.blue_3),
            self.red_3.min(self.blue_3),
        )
    }

    fn apply_delta(&mut self, d_red_4: i64, d_blue_4: i64, d_red_3: i64, d_blue_3: i64) {
        self.red_4 = (self.red_4 as i64 + d_red_4).max(0) as u64;
        self.blue_4 = (self.blue_4 as i64 + d_blue_4).max(0) as u64;
        self.red_3 = (self.red_3 as i64 + d_red_3).max(0) as u64;
        self.blue_3 = (self.blue_3 as i64 + d_blue_3).max(0) as u64;
    }

    fn predicted(&self, d_red_4: i64, d_blue_4: i64, d_red_3: i64, d_blue_3: i64) -> ValidScore {
        let r4 = (self.red_4 as i64 + d_red_4).max(0) as u64;
        let b4 = (self.blue_4 as i64 + d_blue_4).max(0) as u64;
        let r3 = (self.red_3 as i64 + d_red_3).max(0) as u64;
        let b3 = (self.blue_3 as i64 + d_blue_3).max(0) as u64;
        (r4.max(b4), r4.min(b4), r3.max(b3), r3.min(b3))
    }
}

/// Map edge (u, v) with u < v to a flat index.
#[inline]
fn edge_index(u: u32, v: u32, n: u32) -> usize {
    let (u, v) = if u < v { (u, v) } else { (v, u) };
    (u * n - u * (u + 1) / 2 + (v - u - 1)) as usize
}

/// A single-edge move candidate: (u, v, score, d_r4, d_b4, d_r3, d_b3).
type Move1 = (u32, u32, ValidScore, i64, i64, i64, i64);

/// A paired-edge move candidate: (u1, v1, u2, v2, score, d_r4, d_b4, d_r3, d_b3).
type Move2 = (u32, u32, u32, u32, ValidScore, i64, i64, i64, i64);

/// Resolved move: (u1, v1, optional_second_edge, score, d_r4, d_b4, d_r3, d_b3).
type ResolvedMove = (u32, u32, Option<(u32, u32)>, ValidScore, i64, i64, i64, i64);

/// Polish a valid graph via score-aware tabu walk.
///
/// Explores the valid-graph landscape by taking steps that maintain zero
/// violations while optimizing the leaderboard score (4-cliques, then
/// triangle balance). Uses a tabu list to escape score-local-optima.
///
/// When `two_opt` is true and the walk stalls (no best-score improvement
/// for 10 steps), also evaluates paired edge flips where two edges are
/// flipped simultaneously. This allows tunneling through intermediate
/// invalid states to reach better-scoring valid graphs.
///
/// Reports every novel valid graph visited during the walk.
/// Returns the best-scoring valid graph found (or None if no improvement).
#[allow(clippy::too_many_arguments)]
pub fn polish_valid_graph(
    graph: &AdjacencyMatrix,
    k: u32,
    ell: u32,
    max_steps: u32,
    tabu_tenure: u32,
    two_opt: bool,
    known_cids: &mut HashSet<GraphCid>,
    observer: &dyn SearchObserver,
    iteration: u64,
) -> Option<AdjacencyMatrix> {
    if max_steps == 0 {
        return None;
    }

    let n = graph.n();
    let edge_count = (n * (n - 1) / 2) as usize;
    // Activate 2-opt probing after this many steps without best-score improvement
    let stale_threshold: u32 = 10;

    // Current graph state
    let mut current = graph.clone();
    let mut current_comp = current.complement();
    let mut adj_nbrs = NeighborSet::from_adj(&current);
    let mut comp_nbrs = NeighborSet::from_adj(&current_comp);

    // Score tracking (absolute counts for incremental updates)
    let mut score = ScoreState::from_counts(&adj_nbrs, &comp_nbrs, n);

    // Best found
    let mut best_graph = current.clone();
    let mut best_score = score.score_tuple();
    let mut improved = false;
    let mut novel_count: u32 = 0;
    let mut steps_since_improvement: u32 = 0;
    let mut two_opt_taken: u32 = 0;

    // Tabu list: tabu_until[edge_index] = step when tabu expires
    let mut tabu_until: Vec<u32> = vec![0; edge_count];

    // Recount interval to correct incremental drift
    let recount_interval: u32 = 100;

    let mut steps_taken: u32 = 0;

    for step in 1..=max_steps {
        // Periodic full recount to correct drift
        if step % recount_interval == 0 {
            score = ScoreState::from_counts(&adj_nbrs, &comp_nbrs, n);
        }

        // === Phase 1: Evaluate all edges for valid-preserving 1-opt moves ===
        let mut best_move: Option<Move1> = None;
        let mut best_aspiration: Option<Move1> = None;

        let use_2opt = two_opt && steps_since_improvement >= stale_threshold;
        // Collect violating edges for 2-opt probing (only when needed)
        let mut violating_edges: Vec<(u32, u32, i64, i64)> = Vec::new();

        for u in 0..n {
            for v in (u + 1)..n {
                // Validity check: flip must preserve zero violations for target k,ell
                let (dk, de) = violation_delta(&adj_nbrs, &comp_nbrs, k, ell, u, v);
                if dk + de == 0 {
                    // Valid-preserving: evaluate for 1-opt selection
                    let (d_red_4, d_blue_4) = violation_delta(&adj_nbrs, &comp_nbrs, 4, 4, u, v);
                    let (d_red_3, d_blue_3) = violation_delta(&adj_nbrs, &comp_nbrs, 3, 3, u, v);

                    let predicted_tuple = score.predicted(d_red_4, d_blue_4, d_red_3, d_blue_3);

                    let eidx = edge_index(u, v, n);
                    let is_tabu = tabu_until[eidx] > step;

                    // Best non-tabu move
                    if !is_tabu {
                        match &best_move {
                            Some((_, _, s, _, _, _, _)) if predicted_tuple >= *s => {}
                            _ => {
                                best_move = Some((
                                    u,
                                    v,
                                    predicted_tuple,
                                    d_red_4,
                                    d_blue_4,
                                    d_red_3,
                                    d_blue_3,
                                ));
                            }
                        }
                    }

                    // Aspiration: allow tabu if it beats best_score ever found
                    if predicted_tuple < best_score {
                        match &best_aspiration {
                            Some((_, _, s, _, _, _, _)) if predicted_tuple >= *s => {}
                            _ => {
                                best_aspiration = Some((
                                    u,
                                    v,
                                    predicted_tuple,
                                    d_red_4,
                                    d_blue_4,
                                    d_red_3,
                                    d_blue_3,
                                ));
                            }
                        }
                    }
                } else if use_2opt {
                    // Violating edge: collect for 2-opt probing (non-tabu only)
                    let eidx = edge_index(u, v, n);
                    if tabu_until[eidx] <= step {
                        violating_edges.push((u, v, dk, de));
                    }
                }
            }
        }

        // === Phase 2: 2-opt evaluation (paired flips, when stalled) ===
        // For each violating edge A, tentatively flip it, then scan for a
        // compensating edge B such that the pair preserves zero violations.
        let mut best_2opt: Option<Move2> = None;

        if use_2opt && !violating_edges.is_empty() {
            for &(au, av, da_k, da_e) in &violating_edges {
                // Score deltas for edge A in original state
                let (da_r4, da_b4) = violation_delta(&adj_nbrs, &comp_nbrs, 4, 4, au, av);
                let (da_r3, da_b3) = violation_delta(&adj_nbrs, &comp_nbrs, 3, 3, au, av);

                // Tentatively flip A
                adj_nbrs.flip_edge(au, av);
                comp_nbrs.flip_edge(au, av);

                // Scan all non-tabu edges for compensating B
                for bu in 0..n {
                    for bv in (bu + 1)..n {
                        if bu == au && bv == av {
                            continue;
                        }
                        let b_eidx = edge_index(bu, bv, n);
                        if tabu_until[b_eidx] > step {
                            continue;
                        }

                        // Validity: combined flips must restore zero violations.
                        // After A: violations = da_k + da_e. B's delta in post-A state:
                        let (dbk, dbe) = violation_delta(&adj_nbrs, &comp_nbrs, k, ell, bu, bv);
                        if (da_k + da_e) + (dbk + dbe) != 0 {
                            continue;
                        }

                        // Score deltas for B in post-A state
                        let (db_r4, db_b4) = violation_delta(&adj_nbrs, &comp_nbrs, 4, 4, bu, bv);
                        let (db_r3, db_b3) = violation_delta(&adj_nbrs, &comp_nbrs, 3, 3, bu, bv);

                        let predicted = score.predicted(
                            da_r4 + db_r4,
                            da_b4 + db_b4,
                            da_r3 + db_r3,
                            da_b3 + db_b3,
                        );

                        match &best_2opt {
                            Some((_, _, _, _, s, _, _, _, _)) if predicted >= *s => {}
                            _ => {
                                best_2opt = Some((
                                    au,
                                    av,
                                    bu,
                                    bv,
                                    predicted,
                                    da_r4 + db_r4,
                                    da_b4 + db_b4,
                                    da_r3 + db_r3,
                                    da_b3 + db_b3,
                                ));
                            }
                        }
                    }
                }

                // Undo flip A
                adj_nbrs.flip_edge(au, av);
                comp_nbrs.flip_edge(au, av);
            }
        }

        // === Phase 3: Choose best overall move ===
        let chosen_1opt = match (best_aspiration, best_move) {
            (Some(asp), Some(reg)) => {
                if asp.2 < reg.2 {
                    Some(asp)
                } else {
                    Some(reg)
                }
            }
            (Some(asp), None) => Some(asp),
            (None, Some(reg)) => Some(reg),
            (None, None) => None,
        };

        // Unify: (u1, v1, optional second edge, score tuple, combined deltas)
        let chosen: Option<ResolvedMove> = match (chosen_1opt, best_2opt) {
            (Some(one), Some(two)) => {
                if two.4 < one.2 {
                    Some((
                        two.0,
                        two.1,
                        Some((two.2, two.3)),
                        two.4,
                        two.5,
                        two.6,
                        two.7,
                        two.8,
                    ))
                } else {
                    Some((one.0, one.1, None, one.2, one.3, one.4, one.5, one.6))
                }
            }
            (Some(one), None) => Some((one.0, one.1, None, one.2, one.3, one.4, one.5, one.6)),
            (None, Some(two)) => Some((
                two.0,
                two.1,
                Some((two.2, two.3)),
                two.4,
                two.5,
                two.6,
                two.7,
                two.8,
            )),
            (None, None) => None,
        };

        let Some((u1, v1, second_edge, new_tuple, d_r4, d_b4, d_r3, d_b3)) = chosen else {
            break; // No valid moves available
        };

        // === Phase 4: Apply move ===
        // Flip first edge
        let cur1 = current.edge(u1, v1);
        current.set_edge(u1, v1, !cur1);
        current_comp.set_edge(u1, v1, cur1);
        adj_nbrs.flip_edge(u1, v1);
        comp_nbrs.flip_edge(u1, v1);
        let eidx1 = edge_index(u1, v1, n);
        tabu_until[eidx1] = step + tabu_tenure;

        // Flip optional second edge (2-opt)
        if let Some((u2, v2)) = second_edge {
            let cur2 = current.edge(u2, v2);
            current.set_edge(u2, v2, !cur2);
            current_comp.set_edge(u2, v2, cur2);
            adj_nbrs.flip_edge(u2, v2);
            comp_nbrs.flip_edge(u2, v2);
            let eidx2 = edge_index(u2, v2, n);
            tabu_until[eidx2] = step + tabu_tenure;
            two_opt_taken += 1;
        }

        // Update score state
        score.apply_delta(d_r4, d_b4, d_r3, d_b3);

        // Update tabu
        steps_taken = step;

        // Track best
        let score_improved = new_tuple < best_score;
        if score_improved {
            best_graph = current.clone();
            best_score = new_tuple;
            improved = true;
            steps_since_improvement = 0;
        } else {
            steps_since_improvement += 1;
        }

        // Report novel graphs (canonical form + CID dedup).
        // Only canonicalize when score improved or periodically — nauty is expensive.
        if score_improved || step % 20 == 0 {
            let (canonical, _) = canonical_form(&current);
            let cid = extremal_graph::compute_cid(&canonical);
            if known_cids.insert(cid) {
                observer.on_discovery(&RawDiscovery {
                    graph: current.clone(),
                    iteration,
                });
                novel_count += 1;
            }
        }
    }

    debug!(
        steps = steps_taken,
        novel = novel_count,
        two_opt_taken,
        improved,
        best_4c_max = best_score.0,
        best_4c_min = best_score.1,
        best_3c_max = best_score.2,
        best_3c_min = best_score.3,
        "polish: tabu walk complete"
    );

    if improved { Some(best_graph) } else { None }
}

/// Compute the score tuple for a valid graph (for comparison between ILS restarts).
fn score_valid_graph(graph: &AdjacencyMatrix, n: u32) -> ValidScore {
    let adj_nbrs = NeighborSet::from_adj(graph);
    let comp = graph.complement();
    let comp_nbrs = NeighborSet::from_adj(&comp);
    ScoreState::from_counts(&adj_nbrs, &comp_nbrs, n).score_tuple()
}

/// Perturb a valid graph by flipping random validity-preserving edges.
///
/// Finds edges where flipping maintains zero violations, picks `num_flips`
/// at random, applying each sequentially (re-evaluating valid edges after each flip).
fn perturb_valid(
    graph: &AdjacencyMatrix,
    k: u32,
    ell: u32,
    num_flips: u32,
    rng: &mut impl Rng,
) -> AdjacencyMatrix {
    let n = graph.n();
    let mut result = graph.clone();
    let mut comp = result.complement();
    let mut adj_nbrs = NeighborSet::from_adj(&result);
    let mut comp_nbrs = NeighborSet::from_adj(&comp);

    for _ in 0..num_flips {
        // Collect all valid-preserving edges (re-evaluate after each flip)
        let mut valid_edges: Vec<(u32, u32)> = Vec::new();
        for u in 0..n {
            for v in (u + 1)..n {
                let (dk, de) = violation_delta(&adj_nbrs, &comp_nbrs, k, ell, u, v);
                if dk + de == 0 {
                    valid_edges.push((u, v));
                }
            }
        }

        if valid_edges.is_empty() {
            break;
        }

        let &(u, v) = valid_edges.choose(rng).unwrap();
        let cur = result.edge(u, v);
        result.set_edge(u, v, !cur);
        comp.set_edge(u, v, cur);
        adj_nbrs.flip_edge(u, v);
        comp_nbrs.flip_edge(u, v);
    }

    result
}

/// Iterated Local Search polish: repeated polish walks separated by perturbations.
///
/// Chains multiple polish walks from different perturbations of a valid graph,
/// exploring more of the valid-graph landscape than a single walk. Each cycle:
/// 1. Polish (tabu walk) → find local optimum
/// 2. Perturb (random valid-preserving flips) → escape basin
/// 3. Repeat
///
/// Returns the best graph found across all walks, or None if no improvement
/// over the input.
#[allow(clippy::too_many_arguments)]
pub fn ils_polish(
    graph: &AdjacencyMatrix,
    k: u32,
    ell: u32,
    max_steps: u32,
    tabu_tenure: u32,
    two_opt: bool,
    restarts: u32,
    perturb_edges: u32,
    known_cids: &mut HashSet<GraphCid>,
    observer: &dyn SearchObserver,
    iteration: u64,
    rng: &mut impl Rng,
) -> Option<AdjacencyMatrix> {
    // No restarts: fall back to single polish walk
    if restarts == 0 {
        return polish_valid_graph(
            graph,
            k,
            ell,
            max_steps,
            tabu_tenure,
            two_opt,
            known_cids,
            observer,
            iteration,
        );
    }

    let n = graph.n();
    let input_score = score_valid_graph(graph, n);

    // Initial polish walk
    let initial_result = polish_valid_graph(
        graph,
        k,
        ell,
        max_steps,
        tabu_tenure,
        two_opt,
        known_cids,
        observer,
        iteration,
    );

    let mut current = initial_result.as_ref().unwrap_or(graph).clone();
    let mut best_graph: Option<AdjacencyMatrix> = None;
    let mut best_score = input_score;

    if let Some(ref polished) = initial_result {
        let polished_score = score_valid_graph(polished, n);
        if polished_score < best_score {
            best_score = polished_score;
            best_graph = Some(polished.clone());
        }
    }

    // ILS restart loop
    for _restart in 0..restarts {
        let perturbed = perturb_valid(&current, k, ell, perturb_edges, rng);

        let polish_result = polish_valid_graph(
            &perturbed,
            k,
            ell,
            max_steps,
            tabu_tenure,
            two_opt,
            known_cids,
            observer,
            iteration,
        );

        let local_opt = polish_result.as_ref().unwrap_or(&perturbed);
        let local_score = score_valid_graph(local_opt, n);

        if local_score < best_score {
            best_score = local_score;
            best_graph = Some(local_opt.clone());
        }

        // Move to new local optimum for next perturbation (standard ILS)
        current = local_opt.clone();
    }

    debug!(
        restarts,
        perturb_edges,
        improved = best_graph.is_some(),
        best_4c_max = best_score.0,
        best_3c_max = best_score.2,
        "ils_polish: complete"
    );

    best_graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::paley_graph;
    use extremal_worker_api::CollectingObserver;

    #[test]
    fn polish_preserves_validity() {
        // Start from Paley(17) which is valid for R(4,4)
        let graph = paley_graph(17);
        let mut known_cids = HashSet::new();
        let observer = CollectingObserver::new();

        // Add original CID
        let (canonical, _) = canonical_form(&graph);
        let cid = extremal_graph::compute_cid(&canonical);
        known_cids.insert(cid);

        let result =
            polish_valid_graph(&graph, 4, 4, 100, 15, false, &mut known_cids, &observer, 0);

        // If polish returned an improved graph, verify it's still valid
        if let Some(polished) = &result {
            let adj = NeighborSet::from_adj(polished);
            let comp = NeighborSet::from_adj(&polished.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(kc + ei, 0, "polished graph must remain valid for R(4,4)");

            // Score should be <= original
            let orig_adj = NeighborSet::from_adj(&graph);
            let orig_comp = NeighborSet::from_adj(&graph.complement());
            let orig_score = ScoreState::from_counts(&orig_adj, &orig_comp, 17);
            let pol_score = ScoreState::from_counts(&adj, &comp, 17);

            assert!(
                pol_score.score_tuple() <= orig_score.score_tuple(),
                "polished score should be <= original"
            );
        }

        // All discovered graphs must also be valid
        for discovery in observer.drain() {
            let adj = NeighborSet::from_adj(&discovery.graph);
            let comp = NeighborSet::from_adj(&discovery.graph.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(kc + ei, 0, "discovered graph must be valid for R(4,4)");
        }
    }

    #[test]
    fn polish_zero_steps_returns_none() {
        let graph = paley_graph(5);
        let mut known_cids = HashSet::new();
        let observer = CollectingObserver::new();
        let result = polish_valid_graph(&graph, 3, 3, 0, 10, false, &mut known_cids, &observer, 0);
        assert!(result.is_none());
    }

    #[test]
    fn polish_2opt_preserves_validity() {
        // 2-opt polish must never produce invalid graphs
        let graph = paley_graph(17);
        let mut known_cids = HashSet::new();
        let observer = CollectingObserver::new();

        let (canonical, _) = canonical_form(&graph);
        let cid = extremal_graph::compute_cid(&canonical);
        known_cids.insert(cid);

        // Run with 2-opt enabled and enough steps to trigger it (stale_threshold=10)
        let result = polish_valid_graph(&graph, 4, 4, 200, 15, true, &mut known_cids, &observer, 0);

        if let Some(polished) = &result {
            let adj = NeighborSet::from_adj(polished);
            let comp = NeighborSet::from_adj(&polished.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(
                kc + ei,
                0,
                "2-opt polished graph must remain valid for R(4,4)"
            );
        }

        for discovery in observer.drain() {
            let adj = NeighborSet::from_adj(&discovery.graph);
            let comp = NeighborSet::from_adj(&discovery.graph.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(
                kc + ei,
                0,
                "2-opt discovered graph must be valid for R(4,4)"
            );
        }
    }

    #[test]
    fn ils_polish_preserves_validity() {
        use rand::SeedableRng;
        use rand::rngs::SmallRng;

        let graph = paley_graph(17);
        let mut known_cids = HashSet::new();
        let observer = CollectingObserver::new();
        let mut rng = SmallRng::seed_from_u64(42);

        let (canonical, _) = canonical_form(&graph);
        let cid = extremal_graph::compute_cid(&canonical);
        known_cids.insert(cid);

        let result = ils_polish(
            &graph,
            4,
            4,
            50,
            15,
            false, // two_opt
            3,     // restarts
            3,     // perturb edges
            &mut known_cids,
            &observer,
            0,
            &mut rng,
        );

        // If improved, verify validity
        if let Some(polished) = &result {
            let adj = NeighborSet::from_adj(polished);
            let comp = NeighborSet::from_adj(&polished.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(
                kc + ei,
                0,
                "ILS-polished graph must remain valid for R(4,4)"
            );
        }

        // All discovered graphs must be valid
        for discovery in observer.drain() {
            let adj = NeighborSet::from_adj(&discovery.graph);
            let comp = NeighborSet::from_adj(&discovery.graph.complement());
            let kc = count_cliques(&adj, 4, 17);
            let ei = count_cliques(&comp, 4, 17);
            assert_eq!(kc + ei, 0, "ILS-discovered graph must be valid for R(4,4)");
        }
    }

    #[test]
    fn ils_zero_restarts_matches_single_polish() {
        use rand::SeedableRng;
        use rand::rngs::SmallRng;

        let graph = paley_graph(17);
        let mut rng = SmallRng::seed_from_u64(99);

        let mut cids1 = HashSet::new();
        let obs1 = CollectingObserver::new();
        let (canonical, _) = canonical_form(&graph);
        let cid = extremal_graph::compute_cid(&canonical);
        cids1.insert(cid);
        let r1 = polish_valid_graph(&graph, 4, 4, 50, 15, false, &mut cids1, &obs1, 0);

        let mut cids2 = HashSet::new();
        let obs2 = CollectingObserver::new();
        cids2.insert(cid);
        let r2 = ils_polish(
            &graph, 4, 4, 50, 15, false, 0, 3, &mut cids2, &obs2, 0, &mut rng,
        );

        // Both should produce the same result (restarts=0 delegates to polish_valid_graph)
        assert_eq!(r1.is_some(), r2.is_some());
    }
}
