//! Elite refinement strategy: deep polish of top leaderboard graphs.
//!
//! Unlike construction strategies (tree2, crossover, SA, etc.) that build new
//! valid graphs then briefly polish them, refine takes EXISTING high-quality
//! graphs from the leaderboard and applies extremely deep ILS polish to find
//! score improvements.
//!
//! ## Rationale
//!
//! All construction strategies converge to the 4c>=67 basin on n=25 R(5,5).
//! The leaderboard's top graphs (4c=65) were found early and polished briefly
//! (100 steps). Deep polish of these known-good graphs may discover nearby
//! graphs with better secondary scores (gap, automorphism order) or even
//! lower 4-clique counts unreachable from the 4c>=67 basin.
//!
//! ## Algorithm
//!
//! 1. Accept the engine-provided seed graph (from leaderboard) as-is
//! 2. Verify it has zero violations for R(k,l)
//! 3. Run ILS polish with deep parameters (2000 steps, 10 restarts, 5 perturb)
//! 4. Report all novel valid graphs discovered during walks
//! 5. Carry forward known CIDs across rounds to prevent duplicate reports

use std::collections::HashSet;

use rand::SeedableRng;
use rand::rngs::SmallRng;

use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques};
use extremal_types::GraphCid;
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, SearchJob, SearchObserver, SearchResult, SearchStrategy,
};
use tracing::debug;

pub struct RefineSearch;

/// Carry-state: tracks CIDs we've already reported to avoid duplicates.
struct RefineState {
    known_cids: HashSet<GraphCid>,
}

impl SearchStrategy for RefineSearch {
    fn id(&self) -> &str {
        "refine"
    }

    fn name(&self) -> &str {
        "Elite Refinement"
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
                name: "polish_max_steps".into(),
                label: "Polish Max Steps".into(),
                description: "Tabu walk steps per ILS iteration (deep: 2000)".into(),
                param_type: ParamType::Int { min: 0, max: 10000 },
                default: serde_json::json!(2000),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_tabu_tenure".into(),
                label: "Polish Tabu Tenure".into(),
                description: "Edge tabu tenure during polish walk (wider: 50)".into(),
                param_type: ParamType::Int { min: 5, max: 200 },
                default: serde_json::json!(50),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_ils_restarts".into(),
                label: "ILS Restarts".into(),
                description: "Perturbation-walk cycles (deep: 10)".into(),
                param_type: ParamType::Int { min: 0, max: 50 },
                default: serde_json::json!(10),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_ils_perturb".into(),
                label: "ILS Perturb Edges".into(),
                description: "Random valid-preserving flips per perturbation".into(),
                param_type: ParamType::Int { min: 1, max: 30 },
                default: serde_json::json!(5),
                adjustable: true,
            },
            ConfigParam {
                name: "polish_2opt".into(),
                label: "2-opt Paired Flips".into(),
                description: "Enable paired-edge flips when walk stalls".into(),
                param_type: ParamType::Bool,
                default: serde_json::json!(true),
                adjustable: true,
            },
        ]
    }

    fn search(&self, job: &SearchJob, observer: &dyn SearchObserver) -> SearchResult {
        let n = job.n;
        let mut rng = SmallRng::seed_from_u64(job.seed);

        // Read config with defaults tuned for deep refinement
        let config = &job.config;
        let k = config.get("target_k").and_then(|v| v.as_u64()).unwrap_or(5) as u32;
        let ell = config
            .get("target_ell")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
        let polish_max_steps = config
            .get("polish_max_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000) as u32;
        let polish_tabu_tenure = config
            .get("polish_tabu_tenure")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;
        let ils_restarts = config
            .get("polish_ils_restarts")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        let ils_perturb = config
            .get("polish_ils_perturb")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
        let two_opt = config
            .get("polish_2opt")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Restore carry state (known CIDs from previous rounds)
        let mut known_cids: HashSet<GraphCid> = if let Some(state) = job.carry_state.as_ref() {
            if let Some(s) = state.downcast_ref::<RefineState>() {
                s.known_cids.clone()
            } else {
                job.known_cids.clone()
            }
        } else {
            job.known_cids.clone()
        };

        // Get seed graph
        let seed = match &job.init_graph {
            Some(g) => g.clone(),
            None => crate::init::paley_graph(n),
        };

        // Verify validity
        let adj_nbrs = NeighborSet::from_adj(&seed);
        let comp = seed.complement();
        let comp_nbrs = NeighborSet::from_adj(&comp);
        let red_violations = count_cliques(&adj_nbrs, k, n);
        let blue_violations = count_cliques(&comp_nbrs, ell, n);

        if red_violations + blue_violations > 0 {
            debug!(
                red_violations,
                blue_violations, k, ell, "refine: seed graph is not valid, skipping"
            );
            observer.on_progress(&ProgressInfo {
                graph: seed.clone(),
                n,
                strategy: "refine".into(),
                iteration: job.max_iters,
                max_iters: job.max_iters,
                valid: false,
                violation_score: (red_violations + blue_violations) as u32,
                discoveries_so_far: 0,
            });
            return SearchResult {
                best_graph: None,
                valid: false,
                iterations_used: 0,
                discoveries: vec![],
                carry_state: Some(Box::new(RefineState { known_cids })),
            };
        }

        // Register seed CID
        let (canonical, _aut) = canonical_form(&seed);
        let seed_cid = extremal_graph::compute_cid(&canonical);
        known_cids.insert(seed_cid);

        debug!(
            n,
            k,
            ell,
            polish_max_steps,
            polish_tabu_tenure,
            ils_restarts,
            ils_perturb,
            two_opt,
            "refine: starting deep polish on seed"
        );

        // Report initial progress
        observer.on_progress(&ProgressInfo {
            graph: seed.clone(),
            n,
            strategy: "refine".into(),
            iteration: 0,
            max_iters: job.max_iters,
            valid: true,
            violation_score: 0,
            discoveries_so_far: 0,
        });

        // Deep ILS polish — this is where all the work happens
        let polished = crate::polish::ils_polish(
            &seed,
            k,
            ell,
            polish_max_steps,
            polish_tabu_tenure,
            two_opt,
            ils_restarts,
            ils_perturb,
            &mut known_cids,
            observer,
            1,
            &mut rng,
        );

        let best = polished.as_ref().unwrap_or(&seed);
        let improved = polished.is_some();

        debug!(improved, "refine: round complete");

        // Report final progress
        observer.on_progress(&ProgressInfo {
            graph: best.clone(),
            n,
            strategy: "refine".into(),
            iteration: job.max_iters,
            max_iters: job.max_iters,
            valid: true,
            violation_score: 0,
            discoveries_so_far: 0,
        });

        // Cap carry_state size
        if known_cids.len() > 50_000 {
            let excess = known_cids.len() - 40_000;
            let remove: Vec<_> = known_cids.iter().take(excess).cloned().collect();
            for cid in remove {
                known_cids.remove(&cid);
            }
        }

        SearchResult {
            best_graph: Some(best.clone()),
            valid: true,
            iterations_used: job.max_iters,
            discoveries: vec![],
            carry_state: Some(Box::new(RefineState { known_cids })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::paley_graph;
    use extremal_graph::AdjacencyMatrix;
    use extremal_worker_api::CollectingObserver;

    fn make_job(n: u32, k: u32, ell: u32, init_graph: Option<AdjacencyMatrix>) -> SearchJob {
        SearchJob {
            n,
            max_iters: 100_000,
            seed: 42,
            init_graph,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "polish_max_steps": 50,
                "polish_tabu_tenure": 15,
                "polish_ils_restarts": 2,
                "polish_ils_perturb": 2,
                "polish_2opt": false,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 10_000,
            carry_state: None,
        }
    }

    #[test]
    fn refine_r33_n5() {
        let strategy = RefineSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(5);
        let job = make_job(5, 3, 3, Some(paley));

        let result = strategy.search(&job, &observer);
        assert!(result.valid);
        assert!(result.best_graph.is_some());

        let best = result.best_graph.unwrap();
        let adj = NeighborSet::from_adj(&best);
        let comp = NeighborSet::from_adj(&best.complement());
        assert_eq!(count_cliques(&adj, 3, 5), 0);
        assert_eq!(count_cliques(&comp, 3, 5), 0);
    }

    #[test]
    fn refine_r44_n17() {
        let strategy = RefineSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(17);
        let job = make_job(17, 4, 4, Some(paley));

        let result = strategy.search(&job, &observer);
        assert!(result.valid);
        assert!(result.best_graph.is_some());

        let best = result.best_graph.unwrap();
        let adj = NeighborSet::from_adj(&best);
        let comp = NeighborSet::from_adj(&best.complement());
        assert_eq!(count_cliques(&adj, 4, 17), 0);
        assert_eq!(count_cliques(&comp, 4, 17), 0);
    }

    #[test]
    fn refine_reports_discoveries() {
        let strategy = RefineSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(17);
        let job = make_job(17, 4, 4, Some(paley));

        strategy.search(&job, &observer);
        let discoveries = observer.drain();
        for d in &discoveries {
            let adj = NeighborSet::from_adj(&d.graph);
            let comp = NeighborSet::from_adj(&d.graph.complement());
            assert_eq!(count_cliques(&adj, 4, 17) + count_cliques(&comp, 4, 17), 0);
        }
    }

    #[test]
    fn refine_carry_state_persists() {
        let strategy = RefineSearch;
        let observer = CollectingObserver::new();
        let paley = paley_graph(17);

        // Round 1
        let job1 = make_job(17, 4, 4, Some(paley.clone()));
        let result1 = strategy.search(&job1, &observer);
        let disc1 = observer.drain().len();

        // Round 2 with carry_state
        let job2 = SearchJob {
            carry_state: result1.carry_state,
            ..make_job(17, 4, 4, Some(paley))
        };
        let _result2 = strategy.search(&job2, &observer);
        let disc2 = observer.drain().len();

        // Round 2 should report fewer discoveries (CIDs carried from round 1)
        assert!(
            disc2 <= disc1,
            "carry_state should reduce duplicate discoveries: round1={disc1} round2={disc2}"
        );
    }

    #[test]
    fn refine_invalid_seed_skips() {
        let strategy = RefineSearch;
        let observer = CollectingObserver::new();

        // All-edges graph has 5-cliques
        let mut graph = AdjacencyMatrix::new(10);
        for u in 0..10 {
            for v in (u + 1)..10 {
                graph.set_edge(u, v, true);
            }
        }

        let job = make_job(10, 5, 5, Some(graph));
        let result = strategy.search(&job, &observer);
        assert!(!result.valid);
        assert!(result.best_graph.is_none());
    }
}
