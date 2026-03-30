//! Template enumeration strategy: structural backbone analysis of top-scoring
//! leaderboard graphs, then exhaustive/sampled enumeration of "free" edge
//! combinations.
//!
//! ## Rationale
//!
//! All 12 existing strategies hit the 4c>=67 barrier on n=25 R(5,5). The top
//! leaderboard graphs (4c=65) form a small cluster with shared structure.
//! Template analyzes which edges are fixed across all top graphs (backbone)
//! vs variable (free edges), then enumerates combinations within that space.
//!
//! Key difference from refine: refine does sequential local walks FROM a single
//! top graph. Template enumerates the combinatorial space BETWEEN multiple
//! top graphs, finding valid graphs reachable by neither local search nor
//! any single starting point.
//!
//! ## Algorithm
//!
//! 1. **Accumulate**: Collect high-quality seeds across rounds, keeping only best 4c scores
//! 2. **Build**: Identify backbone (edges identical in all pool graphs) and free edges
//! 3. **Enumerate**: Exhaustive if <=25 free edges (2^25=33M), random sampling otherwise
//! 4. **Validate + Polish**: Check R(k,l) validity, polish valid discoveries

use std::collections::HashSet;

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques};
use extremal_types::GraphCid;
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, RawDiscovery, SearchJob, SearchObserver, SearchResult,
    SearchStrategy,
};
use tracing::debug;

pub struct TemplateSearch;

/// Carry state persisted across rounds.
struct TemplateState {
    /// Pool of top-scoring valid graphs: (graph, max(red_4c, blue_4c))
    pool: Vec<(AdjacencyMatrix, u64)>,
    /// Best (lowest) max-4c score in pool
    best_4c: u64,
    /// Base graph for template (first pool entry — backbone edges from this)
    base: Option<AdjacencyMatrix>,
    /// Edges that differ between pool graphs (the "free" edges to enumerate)
    free_edges: Vec<(u32, u32)>,
    /// Total candidates: 2^free_edges.len() (capped at u64::MAX)
    total: u64,
    /// Cursor for exhaustive enumeration (resumes across rounds)
    cursor: u64,
    /// Whether template has been built
    built: bool,
    /// Known CIDs for dedup
    known_cids: HashSet<GraphCid>,
}

impl SearchStrategy for TemplateSearch {
    fn id(&self) -> &str {
        "template"
    }

    fn name(&self) -> &str {
        "Template Enumeration"
    }

    fn config_schema(&self) -> Vec<ConfigParam> {
        vec![
            ConfigParam {
                name: "target_k".into(),
                label: "Target K".into(),
                description: "Red clique size for R(k,l) validity check".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "target_ell".into(),
                label: "Target L".into(),
                description: "Blue clique size for R(k,l) validity check".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "template_pool_size".into(),
                label: "Template Pool Size".into(),
                description: "Min unique seeds to build template (default 3)".into(),
                param_type: ParamType::Int { min: 2, max: 50 },
                default: serde_json::json!(3),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_max_steps".into(),
                label: "Polish Max Steps".into(),
                description: "Tabu walk steps per valid candidate (default 100)".into(),
                param_type: ParamType::Int { min: 0, max: 10000 },
                default: serde_json::json!(100),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_tabu_tenure".into(),
                label: "Polish Tabu Tenure".into(),
                description: "Edge tabu tenure during polish (default 25)".into(),
                param_type: ParamType::Int { min: 5, max: 200 },
                default: serde_json::json!(25),
                adjustable: true,
            },
        ]
    }

    fn search(&self, job: &SearchJob, observer: &dyn SearchObserver) -> SearchResult {
        let n = job.n;
        let mut rng = SmallRng::seed_from_u64(job.seed);
        let config = &job.config;

        let k = config.get("target_k").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
        let ell = config
            .get("target_ell")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
        let pool_min = config
            .get("template_pool_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        let polish_steps = config
            .get("polish_max_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        let polish_tenure = config
            .get("polish_tabu_tenure")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as u32;

        // Restore carry state
        let mut state = if let Some(carry) = job.carry_state.as_ref() {
            if let Some(s) = carry.downcast_ref::<TemplateState>() {
                TemplateState {
                    pool: s.pool.clone(),
                    best_4c: s.best_4c,
                    base: s.base.clone(),
                    free_edges: s.free_edges.clone(),
                    total: s.total,
                    cursor: s.cursor,
                    built: s.built,
                    known_cids: s.known_cids.clone(),
                }
            } else {
                new_state(job)
            }
        } else {
            new_state(job)
        };

        // === Phase 1: Accumulate pool from leaderboard seeds ===
        if let Some(init) = &job.init_graph {
            accumulate_seed(init, k, ell, n, &mut state);
        }

        // === Phase 2: Build template if pool is ready ===
        if !state.built && state.pool.len() >= pool_min {
            build_template(&mut state, n);
        }

        // === Phase 3: Enumerate or sample template combinations ===
        let mut discovery_count = 0u64;
        let mut polished = 0u32;
        let max_polish = 20u32;
        let budget = job.max_iters;

        if state.built && !state.free_edges.is_empty() {
            let base = state.base.as_ref().unwrap().clone();
            let num_free = state.free_edges.len();
            let exhaustive = num_free <= 25;

            let to_eval = if exhaustive {
                budget.min(state.total.saturating_sub(state.cursor))
            } else {
                budget
            };

            debug!(
                num_free,
                total = state.total,
                cursor = state.cursor,
                to_eval,
                exhaustive,
                "template: enumerating"
            );

            observer.on_progress(&ProgressInfo {
                graph: base.clone(),
                n,
                strategy: "template".into(),
                iteration: 0,
                max_iters: budget,
                valid: true,
                violation_score: 0,
                discoveries_so_far: 0,
            });

            for i in 0..to_eval {
                let bits = if exhaustive {
                    state.cursor + i
                } else {
                    rng.r#gen::<u64>()
                };

                // Build candidate: start from base, set each free edge to bit value
                let mut candidate = base.clone();
                for (ei, &(u, v)) in state.free_edges.iter().enumerate() {
                    let val = if ei < 64 {
                        (bits >> ei) & 1 == 1
                    } else {
                        rng.gen_bool(0.5)
                    };
                    candidate.set_edge(u, v, val);
                }

                // Check R(k,l) validity
                let adj_nbrs = NeighborSet::from_adj(&candidate);
                let comp = candidate.complement();
                let comp_nbrs = NeighborSet::from_adj(&comp);
                let red_v = count_cliques(&adj_nbrs, k, n);
                let blue_v = count_cliques(&comp_nbrs, ell, n);

                if red_v + blue_v == 0 {
                    // Valid! Dedup by canonical CID
                    let (canonical, _) = canonical_form(&candidate);
                    let cid = extremal_graph::compute_cid(&canonical);

                    if state.known_cids.insert(cid) {
                        observer.on_discovery(&RawDiscovery {
                            graph: candidate.clone(),
                            iteration: i,
                        });
                        discovery_count += 1;

                        // Polish valid candidate (capped per round)
                        if polished < max_polish && polish_steps > 0 {
                            crate::polish::polish_valid_graph(
                                &candidate,
                                k,
                                ell,
                                polish_steps,
                                polish_tenure,
                                false,
                                &mut state.known_cids,
                                observer,
                                i,
                            );
                            polished += 1;
                        }
                    }
                }
            }

            if exhaustive {
                state.cursor += to_eval;
            }
        } else if !state.built {
            debug!(
                pool_size = state.pool.len(),
                pool_min, "template: accumulating pool"
            );
        } else {
            debug!("template: 0 free edges, all pool graphs identical");
        }

        // Report final progress
        let report_graph = state
            .base
            .clone()
            .or_else(|| state.pool.first().map(|(g, _)| g.clone()))
            .or_else(|| job.init_graph.clone())
            .unwrap_or_else(|| crate::init::paley_graph(n));

        observer.on_progress(&ProgressInfo {
            graph: report_graph.clone(),
            n,
            strategy: "template".into(),
            iteration: budget,
            max_iters: budget,
            valid: !state.pool.is_empty(),
            violation_score: 0,
            discoveries_so_far: discovery_count,
        });

        debug!(
            pool = state.pool.len(),
            built = state.built,
            free = state.free_edges.len(),
            cursor = state.cursor,
            discovery_count,
            polished,
            "template: round complete"
        );

        // Cap known_cids to prevent unbounded growth
        if state.known_cids.len() > 50_000 {
            let excess = state.known_cids.len() - 40_000;
            let remove: Vec<_> = state.known_cids.iter().take(excess).cloned().collect();
            for cid in remove {
                state.known_cids.remove(&cid);
            }
        }

        SearchResult {
            best_graph: Some(report_graph),
            valid: !state.pool.is_empty(),
            iterations_used: budget,
            discoveries: vec![], // reported through observer
            carry_state: Some(Box::new(state)),
        }
    }
}

fn new_state(job: &SearchJob) -> TemplateState {
    TemplateState {
        pool: Vec::new(),
        best_4c: u64::MAX,
        base: None,
        free_edges: Vec::new(),
        total: 0,
        cursor: 0,
        built: false,
        known_cids: job.known_cids.clone(),
    }
}

/// Add a seed graph to the pool if it's valid and ties the best known 4c score.
fn accumulate_seed(graph: &AdjacencyMatrix, k: u32, ell: u32, n: u32, state: &mut TemplateState) {
    let adj_nbrs = NeighborSet::from_adj(graph);
    let comp = graph.complement();
    let comp_nbrs = NeighborSet::from_adj(&comp);

    // Must be R(k,l)-valid
    if count_cliques(&adj_nbrs, k, n) + count_cliques(&comp_nbrs, ell, n) > 0 {
        return;
    }

    // Compute 4-clique max score
    let red_4 = count_cliques(&adj_nbrs, 4, n);
    let blue_4 = count_cliques(&comp_nbrs, 4, n);
    let max_4c = red_4.max(blue_4);

    if max_4c > state.best_4c {
        return; // Worse than pool
    }

    if max_4c < state.best_4c {
        // New best — reset pool and template
        state.pool.clear();
        state.best_4c = max_4c;
        state.built = false;
        state.cursor = 0;
        state.base = None;
        state.free_edges.clear();
    }

    // Dedup by CID
    let (canonical, _) = canonical_form(graph);
    let cid = extremal_graph::compute_cid(&canonical);
    if !state.known_cids.insert(cid) {
        return;
    }

    if state.pool.len() < 50 {
        state.pool.push((graph.clone(), max_4c));
        debug!(
            pool_size = state.pool.len(),
            max_4c, "template: added seed to pool"
        );
    }
}

/// Compute backbone (fixed edges) and free edges from the pool.
fn build_template(state: &mut TemplateState, n: u32) {
    let base = &state.pool[0].0;
    let mut free_edges = Vec::new();

    for u in 0..n {
        for v in (u + 1)..n {
            let base_val = base.edge(u, v);
            if !state.pool.iter().all(|(g, _)| g.edge(u, v) == base_val) {
                free_edges.push((u, v));
            }
        }
    }

    let num_free = free_edges.len();
    let total = if num_free <= 63 {
        1u64 << num_free
    } else {
        u64::MAX
    };

    debug!(
        pool_size = state.pool.len(),
        num_free, total, "template: built"
    );

    state.base = Some(base.clone());
    state.free_edges = free_edges;
    state.total = total;
    state.cursor = 0;
    state.built = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::paley_graph;
    use extremal_worker_api::CollectingObserver;

    fn make_job(
        n: u32,
        k: u32,
        ell: u32,
        init_graph: Option<AdjacencyMatrix>,
        pool_size: u64,
    ) -> SearchJob {
        SearchJob {
            n,
            max_iters: 100_000,
            seed: 42,
            init_graph,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "template_pool_size": pool_size,
                "polish_max_steps": 50,
                "polish_tabu_tenure": 15,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 10_000,
            carry_state: None,
        }
    }

    #[test]
    fn template_r33_n5() {
        let strategy = TemplateSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(5);
        let job = make_job(5, 3, 3, Some(paley), 1);

        let result = strategy.search(&job, &observer);
        assert!(result.valid);
        assert!(result.best_graph.is_some());
    }

    #[test]
    fn template_r44_n17() {
        let strategy = TemplateSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(17);
        let job = make_job(17, 4, 4, Some(paley), 1);

        let result = strategy.search(&job, &observer);
        assert!(result.valid);
        assert!(result.best_graph.is_some());
    }

    #[test]
    fn template_free_edges_computed() {
        // Two valid R(3,3)/n=5 graphs (different C5 labelings)
        let mut g1 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3), (3, 4), (0, 4)] {
            g1.set_edge(u, v, true);
        }
        let mut g2 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 2u32), (2, 4), (1, 4), (1, 3), (0, 3)] {
            g2.set_edge(u, v, true);
        }

        // Both valid R(3,3)
        let a1 = NeighborSet::from_adj(&g1);
        let c1 = NeighborSet::from_adj(&g1.complement());
        assert_eq!(count_cliques(&a1, 3, 5) + count_cliques(&c1, 3, 5), 0);

        let a2 = NeighborSet::from_adj(&g2);
        let c2 = NeighborSet::from_adj(&g2.complement());
        assert_eq!(count_cliques(&a2, 3, 5) + count_cliques(&c2, 3, 5), 0);

        let mut state = TemplateState {
            pool: vec![(g1, 0), (g2, 0)],
            best_4c: 0,
            base: None,
            free_edges: Vec::new(),
            total: 0,
            cursor: 0,
            built: false,
            known_cids: HashSet::new(),
        };
        build_template(&mut state, 5);

        assert!(state.built);
        // These two C5s share no edges — all 10 are free
        assert_eq!(state.free_edges.len(), 10);
        assert_eq!(state.total, 1024); // 2^10
    }

    #[test]
    fn template_partial_backbone() {
        // Two graphs that share most edges but differ in one
        let mut g1 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3)] {
            g1.set_edge(u, v, true);
        }
        let mut g2 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3), (3, 4)] {
            g2.set_edge(u, v, true);
        }

        let mut state = TemplateState {
            pool: vec![(g1, 0), (g2, 0)],
            best_4c: 0,
            base: None,
            free_edges: Vec::new(),
            total: 0,
            cursor: 0,
            built: false,
            known_cids: HashSet::new(),
        };
        build_template(&mut state, 5);

        assert!(state.built);
        assert_eq!(state.free_edges.len(), 1);
        assert_eq!(state.free_edges[0], (3, 4));
        assert_eq!(state.total, 2);
    }

    #[test]
    fn template_carry_state_persists() {
        let strategy = TemplateSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(17);

        // Round 1
        let job1 = make_job(17, 4, 4, Some(paley.clone()), 1);
        let result1 = strategy.search(&job1, &observer);
        assert!(result1.carry_state.is_some());

        // Round 2 with carry_state
        let job2 = SearchJob {
            carry_state: result1.carry_state,
            seed: 43,
            ..make_job(17, 4, 4, Some(paley), 1)
        };
        let result2 = strategy.search(&job2, &observer);
        assert!(result2.carry_state.is_some());
        assert!(result2.valid);
    }

    #[test]
    fn template_enumeration_all_valid() {
        let strategy = TemplateSearch;
        let observer = CollectingObserver::new();

        // Two valid R(3,3)/n=5 graphs
        let mut g1 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 1u32), (1, 2), (2, 3), (3, 4), (0, 4)] {
            g1.set_edge(u, v, true);
        }
        let mut g2 = AdjacencyMatrix::new(5);
        for &(u, v) in &[(0u32, 2u32), (2, 4), (1, 4), (1, 3), (0, 3)] {
            g2.set_edge(u, v, true);
        }

        // Round 1: first seed
        let job1 = make_job(5, 3, 3, Some(g1), 2);
        let result1 = strategy.search(&job1, &observer);

        // Round 2: second seed triggers template build + enumeration
        let job2 = SearchJob {
            carry_state: result1.carry_state,
            init_graph: Some(g2),
            seed: 43,
            ..make_job(5, 3, 3, None, 2)
        };
        let _result2 = strategy.search(&job2, &observer);

        // Every discovery must be valid R(3,3)
        for d in observer.drain() {
            let adj = NeighborSet::from_adj(&d.graph);
            let comp = NeighborSet::from_adj(&d.graph.complement());
            assert_eq!(
                count_cliques(&adj, 3, 5) + count_cliques(&comp, 3, 5),
                0,
                "discovered graph must be valid R(3,3)"
            );
        }
    }
}
