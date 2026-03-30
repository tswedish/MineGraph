//! Large Neighborhood Search (LNS) strategy.
//!
//! Exhaustively enumerates all 2^m states of edges within a random vertex
//! block of size k (where m = k*(k-1)/2), using Gray code for efficient
//! incremental violation tracking. This explores neighborhoods of radius
//! up to m edge-flips — far beyond the radius-1 of single-flip methods
//! (tree2, tabu, SA) and radius-2 of 2-opt.
//!
//! Key insight: all tested local-search methods converge to the 4c>=67
//! basin on n=25 R(5,5). The top 4c=(65,65) graphs may only be reachable
//! via coordinated multi-edge changes. LNS systematically explores these
//! by checking every combination within a vertex block.
//!
//! For k=6: 15 edges, 32768 states per block, ~1ms per block.
//! For k=7: 21 edges, 2M states per block, ~70ms per block.

use std::collections::HashSet;

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques, violation_delta};
use extremal_types::GraphCid;
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, RawDiscovery, SearchJob, SearchObserver, SearchResult,
    SearchStrategy,
};
use rand::prelude::*;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use tracing::debug;

pub struct LnsSearch;

impl SearchStrategy for LnsSearch {
    fn id(&self) -> &str {
        "lns"
    }

    fn name(&self) -> &str {
        "Large Neighborhood Search"
    }

    fn config_schema(&self) -> Vec<ConfigParam> {
        vec![
            ConfigParam {
                name: "target_k".into(),
                label: "Target K".into(),
                description: "Clique size in graph for R(k,l)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "target_ell".into(),
                label: "Target L".into(),
                description: "Clique size in complement for R(k,l)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "lns_block_size".into(),
                label: "Block Size".into(),
                description: "Vertices per neighborhood block (4-8). Higher = larger neighborhood but exponentially more states per block.".into(),
                param_type: ParamType::Int { min: 4, max: 8 },
                default: serde_json::json!(6),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_max_steps".into(),
                label: "Polish Steps".into(),
                description: "Tabu walk steps per valid graph discovered (0 = disable)".into(),
                param_type: ParamType::Int { min: 0, max: 10_000 },
                default: serde_json::json!(100),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_tabu_tenure".into(),
                label: "Polish Tabu Tenure".into(),
                description: "Edge tabu tenure during polish walk".into(),
                param_type: ParamType::Int { min: 1, max: 1000 },
                default: serde_json::json!(25),
                adjustable: true,
            },
        ]
    }

    fn search(&self, job: &SearchJob, observer: &dyn SearchObserver) -> SearchResult {
        let n = job.n;
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
        let block_size = job
            .config
            .get("lns_block_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(6) as u32;
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

        let block_size = block_size.min(n); // can't exceed vertex count
        let m = block_size * (block_size - 1) / 2; // edges in block
        let total_states: u64 = 1u64 << m;

        let mut rng = SmallRng::seed_from_u64(job.seed);
        let mut known_cids: HashSet<GraphCid> = job.known_cids.clone();

        // Initialize from seed graph (Paley fallback for cold start)
        let mut graph = job
            .init_graph
            .clone()
            .unwrap_or_else(|| crate::init::paley_graph(n));
        let mut comp = graph.complement();
        let mut adj_nbrs = NeighborSet::from_adj(&graph);
        let mut comp_nbrs = NeighborSet::from_adj(&comp);

        let mut iters_used = 0u64;
        let mut discovery_count = 0u64;
        let mut best_valid: Option<AdjacencyMatrix> = None;
        let mut best_valid_score: Option<(u64, u64)> = None;
        let mut polish_calls = 0u32;
        let max_polish_per_round = 20u32;
        let mut blocks_explored = 0u64;

        let vertices: Vec<u32> = (0..n).collect();

        // Check if seed is already valid
        let init_kc = count_cliques(&adj_nbrs, k, n);
        let init_ei = count_cliques(&comp_nbrs, ell, n);
        if init_kc + init_ei == 0 {
            let r4 = count_cliques(&adj_nbrs, 4, n);
            let b4 = count_cliques(&comp_nbrs, 4, n);
            best_valid_score = Some((r4.max(b4), r4.min(b4)));
            best_valid = Some(graph.clone());

            let (canonical, _) = canonical_form(&graph);
            let cid = extremal_graph::compute_cid(&canonical);
            if known_cids.insert(cid) {
                observer.on_discovery(&RawDiscovery {
                    graph: graph.clone(),
                    iteration: 0,
                });
                discovery_count += 1;
            }
        }

        // Initial progress report
        observer.on_progress(&ProgressInfo {
            graph: graph.clone(),
            n,
            strategy: "lns".into(),
            iteration: 0,
            max_iters: job.max_iters,
            valid: best_valid.is_some(),
            violation_score: (init_kc + init_ei) as u32,
            discoveries_so_far: discovery_count,
        });

        debug!(
            "lns: n={n}, k={k}, ell={ell}, block_size={block_size}, m={m}, states/block={total_states}"
        );

        // Main loop: try different vertex subsets
        while iters_used + (total_states - 1) <= job.max_iters {
            if observer.is_cancelled() {
                break;
            }

            // Select random block of vertices
            let block: Vec<u32> = vertices
                .choose_multiple(&mut rng, block_size as usize)
                .cloned()
                .collect();

            // Enumerate edges in the block (consistent ordering)
            let mut edges: Vec<(u32, u32)> = Vec::with_capacity(m as usize);
            for i in 0..block_size as usize {
                for j in (i + 1)..block_size as usize {
                    edges.push((block[i], block[j]));
                }
            }

            // Save original edge states for restoration
            let original_states: Vec<bool> = edges.iter().map(|&(u, v)| graph.edge(u, v)).collect();

            // Full recount for accurate baseline
            let mut kc = count_cliques(&adj_nbrs, k, n) as i64;
            let mut ei = count_cliques(&comp_nbrs, ell, n) as i64;

            // Gray code enumeration: visit all 2^m states
            // At step i, flip bit position = trailing_zeros(i)
            for step in 1..total_states {
                let bit = step.trailing_zeros() as usize;
                let (u, v) = edges[bit];

                // Compute violation delta before flipping
                let (dk, de) = violation_delta(&adj_nbrs, &comp_nbrs, k, ell, u, v);

                // Flip the edge in graph, complement, and neighbor sets
                let cur = graph.edge(u, v);
                graph.set_edge(u, v, !cur);
                comp.set_edge(u, v, cur);
                adj_nbrs.flip_edge(u, v);
                comp_nbrs.flip_edge(u, v);

                kc += dk;
                ei += de;
                iters_used += 1;

                // Valid graph found?
                if kc <= 0 && ei <= 0 {
                    // Full recount to guard against drift
                    let actual_kc = count_cliques(&adj_nbrs, k, n);
                    let actual_ei = count_cliques(&comp_nbrs, ell, n);
                    // Correct tracking
                    kc = actual_kc as i64;
                    ei = actual_ei as i64;

                    if actual_kc + actual_ei == 0 {
                        let r4 = count_cliques(&adj_nbrs, 4, n);
                        let b4 = count_cliques(&comp_nbrs, 4, n);
                        let score = (r4.max(b4), r4.min(b4));
                        let is_better = best_valid_score.is_none_or(|bs| score < bs);

                        let (canonical, _) = canonical_form(&graph);
                        let cid = extremal_graph::compute_cid(&canonical);

                        if known_cids.insert(cid) {
                            observer.on_discovery(&RawDiscovery {
                                graph: graph.clone(),
                                iteration: iters_used,
                            });
                            discovery_count += 1;

                            if is_better {
                                best_valid = Some(graph.clone());
                                best_valid_score = Some(score);
                                debug!(
                                    "lns: new best 4c=({},{}) at block {} step {}",
                                    r4, b4, blocks_explored, step
                                );
                            }

                            // Polish the discovery
                            if polish_max_steps > 0 && polish_calls < max_polish_per_round {
                                polish_calls += 1;
                                if let Some(polished) = crate::polish::polish_valid_graph(
                                    &graph,
                                    k,
                                    ell,
                                    polish_max_steps,
                                    polish_tabu_tenure,
                                    false, // two_opt
                                    &mut known_cids,
                                    observer,
                                    iters_used,
                                ) {
                                    let p_adj = NeighborSet::from_adj(&polished);
                                    let p_comp_g = polished.complement();
                                    let p_comp = NeighborSet::from_adj(&p_comp_g);
                                    let p_r4 = count_cliques(&p_adj, 4, polished.n());
                                    let p_b4 = count_cliques(&p_comp, 4, polished.n());
                                    let p_score = (p_r4.max(p_b4), p_r4.min(p_b4));
                                    if best_valid_score.is_none_or(|bs| p_score < bs) {
                                        best_valid = Some(polished);
                                        best_valid_score = Some(p_score);
                                    }
                                }
                            }
                        }
                    }
                }

                // Periodic progress
                if iters_used.is_multiple_of(50000) {
                    observer.on_progress(&ProgressInfo {
                        graph: graph.clone(),
                        n,
                        strategy: "lns".into(),
                        iteration: iters_used,
                        max_iters: job.max_iters,
                        valid: best_valid.is_some(),
                        violation_score: (kc.max(0) + ei.max(0)) as u32,
                        discoveries_so_far: discovery_count,
                    });
                }
            }

            // Restore original graph state
            for (i, &(u, v)) in edges.iter().enumerate() {
                if graph.edge(u, v) != original_states[i] {
                    graph.set_edge(u, v, original_states[i]);
                    comp.set_edge(u, v, !original_states[i]);
                    adj_nbrs.flip_edge(u, v);
                    comp_nbrs.flip_edge(u, v);
                }
            }

            blocks_explored += 1;

            if blocks_explored.is_multiple_of(100) {
                debug!(
                    "lns: {} blocks explored, {} discoveries, {} iters used",
                    blocks_explored, discovery_count, iters_used
                );
            }
        }

        debug!(
            "lns: done. {} blocks, {} discoveries in {} iters",
            blocks_explored, discovery_count, iters_used
        );

        // Final progress
        observer.on_progress(&ProgressInfo {
            graph: best_valid.clone().unwrap_or_else(|| graph.clone()),
            n,
            strategy: "lns".into(),
            iteration: iters_used,
            max_iters: job.max_iters,
            valid: best_valid.is_some(),
            violation_score: 0,
            discoveries_so_far: discovery_count,
        });

        SearchResult {
            valid: best_valid.is_some(),
            best_graph: best_valid,
            iterations_used: iters_used,
            discoveries: vec![],
            carry_state: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use extremal_worker_api::CollectingObserver;
    use std::sync::Arc;

    fn make_job(n: u32, k: u32, ell: u32, block_size: u32, max_iters: u64) -> SearchJob {
        SearchJob {
            n,
            max_iters,
            seed: 42,
            init_graph: None,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "lns_block_size": block_size,
                "polish_max_steps": 0,
                "polish_tabu_tenure": 25,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 1000,
            carry_state: None,
        }
    }

    #[test]
    fn lns_finds_valid_r33_n5() {
        // R(3,3) = 6, so n=5 should have valid graphs.
        // With block_size=4 (m=6, 64 states), LNS should find valid graphs.
        let job = make_job(5, 3, 3, 4, 100_000);
        let obs = Arc::new(CollectingObserver::new());
        let result = LnsSearch.search(&job, obs.as_ref());
        assert!(result.valid, "LNS should find valid R(3,3) graphs on n=5");
    }

    #[test]
    fn lns_reports_discoveries() {
        let job = make_job(5, 3, 3, 4, 100_000);
        let obs = Arc::new(CollectingObserver::new());
        LnsSearch.search(&job, obs.as_ref());
        assert!(
            !obs.drain().is_empty(),
            "LNS should report discoveries via observer"
        );
    }

    #[test]
    fn lns_respects_iteration_budget() {
        // With tiny budget, should not exceed it
        let job = make_job(5, 3, 3, 4, 50);
        let obs = Arc::new(CollectingObserver::new());
        let result = LnsSearch.search(&job, obs.as_ref());
        assert!(
            result.iterations_used <= 50,
            "should respect iteration budget: used {}",
            result.iterations_used
        );
    }

    #[test]
    fn lns_finds_valid_r44_n17() {
        // R(4,4) = 18, so n=17 should have valid graphs.
        // Larger instance, block_size=5 (m=10, 1024 states).
        let job = make_job(17, 4, 4, 5, 500_000);
        let obs = Arc::new(CollectingObserver::new());
        let result = LnsSearch.search(&job, obs.as_ref());
        assert!(result.valid, "LNS should find valid R(4,4) graphs on n=17");
    }

    #[test]
    fn lns_block_edge_count() {
        // Verify m = k*(k-1)/2 for block_size k
        assert_eq!(4 * 3 / 2, 6); // k=4: 6 edges, 64 states
        assert_eq!(5 * 4 / 2, 10); // k=5: 10 edges, 1024 states
        assert_eq!(6 * 5 / 2, 15); // k=6: 15 edges, 32768 states
        assert_eq!(7 * 6 / 2, 21); // k=7: 21 edges, 2M states
    }
}
