//! Seidel switching search strategy for Ramsey graph exploration.
//!
//! Seidel switching with respect to a vertex subset S ⊆ V flips ALL edges
//! between S and V\S simultaneously. This is a well-known operation in
//! algebraic graph theory (two-graph theory) that has been used in Ramsey
//! research to discover new constructions.
//!
//! ## Key properties
//!
//! - **Structured perturbation**: Unlike random single/paired edge flips,
//!   switching produces correlated multi-edge changes (|S|=1 → n-1 edges,
//!   |S|=2 → 2n-6 edges, etc.)
//! - **Involutory**: Switching twice by the same set S returns the original graph
//! - **Two-graph preservation**: Switching preserves the switching equivalence
//!   class, a well-studied algebraic invariant
//! - **Fundamentally different**: Not reachable by any k-flip local search
//!   (changes O(n) edges at once in a structured pattern)
//!
//! ## Algorithm
//!
//! 1. Start from leaderboard seed (or Paley graph)
//! 2. Exhaustively enumerate switching sets of size 1..max_switch_size
//! 3. For each: apply switching, check R(k,ℓ) validity
//! 4. Polish and report valid discoveries
//! 5. Also try random switching compositions: switch by S1, then by S2

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques};
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, RawDiscovery, SearchJob, SearchObserver, SearchResult,
    SearchStrategy,
};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::collections::HashSet;
use tracing::{debug, info};

pub struct SeidelSearch;

impl SearchStrategy for SeidelSearch {
    fn id(&self) -> &str {
        "seidel"
    }

    fn name(&self) -> &str {
        "Seidel Switching"
    }

    fn config_schema(&self) -> Vec<ConfigParam> {
        vec![
            ConfigParam {
                name: "target_k".into(),
                label: "Target K".into(),
                description: "Clique size to avoid in graph (red)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "target_ell".into(),
                label: "Target Ell".into(),
                description: "Clique size to avoid in complement (blue)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "max_switch_size".into(),
                label: "Max Switch Size".into(),
                description: "Maximum switching set size for exhaustive enumeration (1-4)".into(),
                param_type: ParamType::Int { min: 1, max: 6 },
                default: serde_json::json!(3),
                adjustable: true,
            },
            ConfigParam {
                name: "random_compositions".into(),
                label: "Random Compositions".into(),
                description: "Number of random switching compositions to try per round".into(),
                param_type: ParamType::Int {
                    min: 0,
                    max: 100_000,
                },
                default: serde_json::json!(5000),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_max_steps".into(),
                label: "Polish Max Steps".into(),
                description: "Maximum steps in score-aware polish per valid graph".into(),
                param_type: ParamType::Int { min: 0, max: 5_000 },
                default: serde_json::json!(100),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_tabu_tenure".into(),
                label: "Polish Tabu Tenure".into(),
                description: "Steps an edge stays tabu during polish".into(),
                param_type: ParamType::Int { min: 5, max: 100 },
                default: serde_json::json!(25),
                adjustable: true,
            },
        ]
    }

    fn search(&self, job: &SearchJob, observer: &dyn SearchObserver) -> SearchResult {
        let k = job
            .config
            .get("target_k")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
        let ell = job
            .config
            .get("target_ell")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
        let max_switch_size = job
            .config
            .get("max_switch_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;
        let random_compositions = job
            .config
            .get("random_compositions")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000) as u32;
        let polish_max_steps = job
            .config
            .get("polish_max_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        let polish_tabu_tenure = job
            .config
            .get("polish_tabu_tenure")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as u32;

        let n = job.n;
        let mut rng = SmallRng::seed_from_u64(job.seed);

        // Get seed graph
        let seed = match &job.init_graph {
            Some(g) => g.clone(),
            None => {
                info!("seidel: no init_graph provided, using empty graph");
                return SearchResult {
                    valid: false,
                    best_graph: None,
                    iterations_used: 0,
                    discoveries: Vec::new(),
                    carry_state: None,
                };
            }
        };

        let mut known_cids = job.known_cids.clone();
        let mut discovery_count: u64 = 0;
        let mut valid_count: u64 = 0;
        let mut best_valid: Option<AdjacencyMatrix> = None;
        let mut best_valid_score: Option<(u64, u64)> = None;
        let mut polish_calls: u32 = 0;
        let max_polish_per_round: u32 = 50;
        let mut iteration: u64 = 0;

        observer.on_progress(&ProgressInfo {
            graph: seed.clone(),
            n,
            strategy: "seidel".to_string(),
            iteration: 0,
            max_iters: job.max_iters,
            valid: false,
            violation_score: 0,
            discoveries_so_far: 0,
        });

        // Phase 1: Exhaustive enumeration of small switching sets
        // For size s, enumerate all C(n, s) subsets
        for size in 1..=max_switch_size.min(n) {
            if observer.is_cancelled() {
                break;
            }

            let mut subset = Vec::with_capacity(size as usize);
            enumerate_subsets(
                n,
                size,
                &mut subset,
                0,
                &seed,
                k,
                ell,
                polish_max_steps,
                polish_tabu_tenure,
                &mut known_cids,
                observer,
                &mut discovery_count,
                &mut valid_count,
                &mut best_valid,
                &mut best_valid_score,
                &mut polish_calls,
                max_polish_per_round,
                &mut iteration,
                job.max_iters,
            );
        }

        info!(
            valid_count,
            discovery_count,
            iteration,
            phase = "exhaustive",
            "seidel: exhaustive switching complete"
        );

        // Phase 2: Random switching compositions
        // Apply two random switching sets in sequence to explore deeper
        if random_compositions > 0 && !observer.is_cancelled() {
            let remaining_budget = job.max_iters.saturating_sub(iteration);
            let compositions = (random_compositions as u64).min(remaining_budget);

            for _ in 0..compositions {
                if observer.is_cancelled() || iteration >= job.max_iters {
                    break;
                }
                iteration += 1;

                // Pick two random switching sets and compose them
                let size1 = rng.gen_range(1..=max_switch_size.min(n / 2).max(1));
                let set1 = random_subset(n, size1, &mut rng);
                let size2 = rng.gen_range(1..=max_switch_size.min(n / 2).max(1));
                let set2 = random_subset(n, size2, &mut rng);

                // Apply switching by set1, then by set2
                let switched = seidel_switch(&seidel_switch(&seed, &set1), &set2);

                check_and_report(
                    &switched,
                    n,
                    k,
                    ell,
                    polish_max_steps,
                    polish_tabu_tenure,
                    &mut known_cids,
                    observer,
                    &mut discovery_count,
                    &mut valid_count,
                    &mut best_valid,
                    &mut best_valid_score,
                    &mut polish_calls,
                    max_polish_per_round,
                    iteration,
                );

                if iteration.is_multiple_of(500) {
                    observer.on_progress(&ProgressInfo {
                        graph: best_valid.clone().unwrap_or_else(|| switched.clone()),
                        n,
                        strategy: "seidel".to_string(),
                        iteration,
                        max_iters: job.max_iters,
                        valid: best_valid.is_some(),
                        violation_score: 0,
                        discoveries_so_far: discovery_count,
                    });
                }
            }
        }

        info!(
            valid_count,
            discovery_count,
            polish_calls,
            iteration,
            best_4c = ?best_valid_score.map(|(m, _)| m),
            "seidel: round complete"
        );

        observer.on_progress(&ProgressInfo {
            graph: best_valid.clone().unwrap_or_else(|| seed.clone()),
            n,
            strategy: "seidel".to_string(),
            iteration,
            max_iters: job.max_iters,
            valid: best_valid.is_some(),
            violation_score: 0,
            discoveries_so_far: discovery_count,
        });

        SearchResult {
            valid: best_valid.is_some(),
            best_graph: best_valid,
            iterations_used: iteration,
            discoveries: Vec::new(),
            carry_state: None,
        }
    }
}

/// Apply Seidel switching to graph G with respect to vertex set S.
/// For every pair (u, v) where u ∈ S and v ∉ S, flip the edge.
fn seidel_switch(graph: &AdjacencyMatrix, switching_set: &[u32]) -> AdjacencyMatrix {
    let n = graph.n();
    let mut result = graph.clone();
    let mut in_set = vec![false; n as usize];
    for &v in switching_set {
        in_set[v as usize] = true;
    }

    for u in 0..n {
        if !in_set[u as usize] {
            continue;
        }
        for v in 0..n {
            if in_set[v as usize] || u == v {
                continue;
            }
            let (lo, hi) = (u.min(v), u.max(v));
            let current = result.edge(lo, hi);
            result.set_edge(lo, hi, !current);
        }
    }
    result
}

/// Generate a random subset of {0, ..., n-1} of given size.
fn random_subset(n: u32, size: u32, rng: &mut SmallRng) -> Vec<u32> {
    let mut vertices: Vec<u32> = (0..n).collect();
    // Fisher-Yates partial shuffle
    let size = size.min(n) as usize;
    for i in 0..size {
        let j = rng.gen_range(i..vertices.len());
        vertices.swap(i, j);
    }
    vertices[..size].to_vec()
}

/// Recursively enumerate all subsets of {0..n-1} of given size,
/// applying Seidel switching and checking validity for each.
#[allow(clippy::too_many_arguments)]
fn enumerate_subsets(
    n: u32,
    size: u32,
    current: &mut Vec<u32>,
    start: u32,
    seed: &AdjacencyMatrix,
    k: u32,
    ell: u32,
    polish_max_steps: u32,
    polish_tabu_tenure: u32,
    known_cids: &mut HashSet<extremal_types::GraphCid>,
    observer: &dyn SearchObserver,
    discovery_count: &mut u64,
    valid_count: &mut u64,
    best_valid: &mut Option<AdjacencyMatrix>,
    best_valid_score: &mut Option<(u64, u64)>,
    polish_calls: &mut u32,
    max_polish_per_round: u32,
    iteration: &mut u64,
    max_iters: u64,
) {
    if current.len() == size as usize {
        if observer.is_cancelled() || *iteration >= max_iters {
            return;
        }
        *iteration += 1;

        let switched = seidel_switch(seed, current);
        check_and_report(
            &switched,
            n,
            k,
            ell,
            polish_max_steps,
            polish_tabu_tenure,
            known_cids,
            observer,
            discovery_count,
            valid_count,
            best_valid,
            best_valid_score,
            polish_calls,
            max_polish_per_round,
            *iteration,
        );

        if (*iteration).is_multiple_of(200) {
            observer.on_progress(&ProgressInfo {
                graph: best_valid.clone().unwrap_or_else(|| switched.clone()),
                n,
                strategy: "seidel".to_string(),
                iteration: *iteration,
                max_iters,
                valid: best_valid.is_some(),
                violation_score: 0,
                discoveries_so_far: *discovery_count,
            });
        }
        return;
    }

    for v in start..n {
        if observer.is_cancelled() || *iteration >= max_iters {
            return;
        }
        current.push(v);
        enumerate_subsets(
            n,
            size,
            current,
            v + 1,
            seed,
            k,
            ell,
            polish_max_steps,
            polish_tabu_tenure,
            known_cids,
            observer,
            discovery_count,
            valid_count,
            best_valid,
            best_valid_score,
            polish_calls,
            max_polish_per_round,
            iteration,
            max_iters,
        );
        current.pop();
    }
}

/// Check a switched graph for R(k,ℓ) validity, canonicalize, dedup, polish, report.
#[allow(clippy::too_many_arguments)]
fn check_and_report(
    graph: &AdjacencyMatrix,
    n: u32,
    k: u32,
    ell: u32,
    polish_max_steps: u32,
    polish_tabu_tenure: u32,
    known_cids: &mut HashSet<extremal_types::GraphCid>,
    observer: &dyn SearchObserver,
    discovery_count: &mut u64,
    valid_count: &mut u64,
    best_valid: &mut Option<AdjacencyMatrix>,
    best_valid_score: &mut Option<(u64, u64)>,
    polish_calls: &mut u32,
    max_polish_per_round: u32,
    iteration: u64,
) {
    let adj_nbrs = NeighborSet::from_adj(graph);
    let comp = graph.complement();
    let comp_nbrs = NeighborSet::from_adj(&comp);

    let kc = count_cliques(&adj_nbrs, k, n);
    let ei = count_cliques(&comp_nbrs, ell, n);

    if kc + ei != 0 {
        return;
    }

    *valid_count += 1;

    let red_4 = count_cliques(&adj_nbrs, 4, n);
    let blue_4 = count_cliques(&comp_nbrs, 4, n);
    let max_4c = red_4.max(blue_4);
    let min_4c = red_4.min(blue_4);

    let (canonical, _) = canonical_form(graph);
    let cid = extremal_graph::compute_cid(&canonical);

    if !known_cids.insert(cid) {
        return; // Already seen
    }

    observer.on_discovery(&RawDiscovery {
        graph: graph.clone(),
        iteration,
    });
    *discovery_count += 1;

    let is_better = match *best_valid_score {
        Some((bmax, bmin)) => (max_4c, min_4c) < (bmax, bmin),
        None => true,
    };
    if is_better {
        *best_valid = Some(graph.clone());
        *best_valid_score = Some((max_4c, min_4c));
        info!(
            red_4,
            blue_4, max_4c, min_4c, "seidel: new best valid graph"
        );
    }

    // Polish for score improvement
    if polish_max_steps > 0 && *polish_calls < max_polish_per_round {
        *polish_calls += 1;
        if let Some(polished) = crate::polish::polish_valid_graph(
            graph,
            k,
            ell,
            polish_max_steps,
            polish_tabu_tenure,
            false,
            known_cids,
            observer,
            iteration,
        ) {
            let p_adj = NeighborSet::from_adj(&polished);
            let p_comp_g = polished.complement();
            let p_comp = NeighborSet::from_adj(&p_comp_g);
            let p_r4 = count_cliques(&p_adj, 4, polished.n());
            let p_b4 = count_cliques(&p_comp, 4, polished.n());
            let p_max = p_r4.max(p_b4);
            let p_min = p_r4.min(p_b4);
            let polished_better = match *best_valid_score {
                Some((bmax, bmin)) => (p_max, p_min) < (bmax, bmin),
                None => true,
            };
            if polished_better {
                *best_valid = Some(polished);
                *best_valid_score = Some((p_max, p_min));
            }
        }
    }

    debug!(
        red_4,
        blue_4,
        valid = *valid_count,
        discoveries = *discovery_count,
        "seidel: valid graph found"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use extremal_worker_api::{CollectingObserver, NoOpObserver};
    use std::collections::HashSet;

    fn make_job(n: u32, k: u32, ell: u32, init: Option<AdjacencyMatrix>) -> SearchJob {
        SearchJob {
            n,
            max_iters: 1_000_000,
            seed: 42,
            init_graph: init,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "max_switch_size": 3,
                "random_compositions": 1000,
                "polish_max_steps": 0,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 10_000,
            carry_state: None,
        }
    }

    /// Build the Petersen graph complement (a known R(3,3)-valid graph on 5 vertices).
    fn r33_seed() -> AdjacencyMatrix {
        // C5 cycle: 0-1-2-3-4-0
        let mut g = AdjacencyMatrix::new(5);
        g.set_edge(0, 1, true);
        g.set_edge(1, 2, true);
        g.set_edge(2, 3, true);
        g.set_edge(3, 4, true);
        g.set_edge(0, 4, true);
        g
    }

    #[test]
    fn seidel_switch_involution() {
        // Switching twice by the same set returns the original graph
        let g = r33_seed();
        let set = vec![0, 2];
        let switched = seidel_switch(&g, &set);
        let back = seidel_switch(&switched, &set);
        assert_eq!(g.num_edges(), back.num_edges());
        for i in 0..5 {
            for j in (i + 1)..5 {
                assert_eq!(g.edge(i, j), back.edge(i, j));
            }
        }
    }

    #[test]
    fn seidel_switch_changes_edges() {
        let g = r33_seed();
        let set = vec![0];
        let switched = seidel_switch(&g, &set);
        // Vertex 0 has edges to 1 and 4 in C5
        // After switching {0}: all edges from 0 are flipped
        // 0-1: was true → false, 0-2: was false → true
        // 0-3: was false → true, 0-4: was true → false
        assert!(!switched.edge(0, 1));
        assert!(switched.edge(0, 2));
        assert!(switched.edge(0, 3));
        assert!(!switched.edge(0, 4));
        // Edges not involving vertex 0 are unchanged
        assert!(switched.edge(1, 2));
        assert!(switched.edge(2, 3));
        assert!(switched.edge(3, 4));
    }

    #[test]
    fn seidel_empty_set_is_identity() {
        let g = r33_seed();
        let switched = seidel_switch(&g, &[]);
        for i in 0..5 {
            for j in (i + 1)..5 {
                assert_eq!(g.edge(i, j), switched.edge(i, j));
            }
        }
    }

    #[test]
    fn seidel_finds_r33_n5() {
        let seed = r33_seed();
        let job = make_job(5, 3, 3, Some(seed));
        let observer = CollectingObserver::new();
        let result = SeidelSearch.search(&job, &observer);
        // The seed itself is valid, and some switchings may produce valid graphs too
        assert!(result.valid, "should find valid R(3,3) graph");
        let discoveries = observer.drain();
        assert!(
            !discoveries.is_empty(),
            "should discover at least the seed itself"
        );
    }

    #[test]
    fn seidel_finds_r44_n17() {
        // Use Paley(17) as seed
        let seed = crate::init::paley_graph(17);
        let job = make_job(17, 4, 4, Some(seed));
        let observer = CollectingObserver::new();
        let result = SeidelSearch.search(&job, &observer);
        assert!(
            result.valid,
            "should find valid R(4,4) graph on 17 vertices"
        );
    }

    #[test]
    fn seidel_no_init_graph_returns_early() {
        let job = make_job(5, 3, 3, None);
        let result = SeidelSearch.search(&job, &NoOpObserver);
        assert!(!result.valid);
        assert_eq!(result.iterations_used, 0);
    }

    #[test]
    fn seidel_reports_discoveries() {
        let seed = r33_seed();
        let job = make_job(5, 3, 3, Some(seed));
        let observer = CollectingObserver::new();
        SeidelSearch.search(&job, &observer);
        let discoveries = observer.drain();
        // Should report each unique valid graph found
        assert!(
            !discoveries.is_empty(),
            "should report at least 1 discovery"
        );
    }

    #[test]
    fn random_subset_correct_size() {
        let mut rng = SmallRng::seed_from_u64(42);
        for size in 1..=5 {
            let s = random_subset(10, size, &mut rng);
            assert_eq!(s.len(), size as usize);
            // All elements should be in range
            for &v in &s {
                assert!(v < 10);
            }
            // No duplicates
            let unique: HashSet<u32> = s.iter().copied().collect();
            assert_eq!(unique.len(), s.len());
        }
    }
}
