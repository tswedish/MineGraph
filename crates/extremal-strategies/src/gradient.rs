//! Gradient Descent strategy for Ramsey graph optimization.
//!
//! Two-phase approach that separates 4-clique optimization from validity:
//!
//! ## Phase 1 (Descent)
//! Greedily minimize max(red_4c, blue_4c) by directed edge flips, ignoring
//! violations entirely. Uses a tabu list to prevent cycling. This pushes the
//! graph deep into low-4c territory, far beyond where constrained methods can
//! reach.
//!
//! ## Phase 2 (Repair)
//! Greedily minimize violations (k-cliques + ell-cliques) via tabu search,
//! bringing the graph back to validity. If repair succeeds, the resulting
//! valid graph may be in a different 4c basin than methods that never leave
//! valid space.
//!
//! ## Key difference from SA
//! SA mixes 4c and violation objectives with random moves. Gradient uses
//! DIRECTED moves — always picking the edge that best reduces the target
//! metric. Phase separation ensures full-commitment descent before repair.

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::SmallRng;

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques, violation_delta};
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, RawDiscovery, SearchJob, SearchObserver, SearchResult,
    SearchStrategy,
};
use tracing::debug;

pub struct GradientDescent;

/// Map edge (u, v) with u < v to a flat index.
#[inline]
fn edge_index(u: u32, v: u32, n: u32) -> usize {
    let (u, v) = if u < v { (u, v) } else { (v, u) };
    (u * n - u * (u + 1) / 2 + (v - u - 1)) as usize
}

impl SearchStrategy for GradientDescent {
    fn id(&self) -> &str {
        "gradient"
    }

    fn name(&self) -> &str {
        "Gradient Descent"
    }

    fn config_schema(&self) -> Vec<ConfigParam> {
        vec![
            ConfigParam {
                name: "gradient_descent_steps".into(),
                label: "Descent Steps".into(),
                description: "Max steps in 4c descent phase".into(),
                param_type: ParamType::Int {
                    min: 10,
                    max: 10_000,
                },
                default: serde_json::json!(200),
                adjustable: true,
            },
            ConfigParam {
                name: "gradient_repair_steps".into(),
                label: "Repair Steps".into(),
                description: "Max steps in violation repair phase".into(),
                param_type: ParamType::Int {
                    min: 100,
                    max: 100_000,
                },
                default: serde_json::json!(5000),
                adjustable: true,
            },
            ConfigParam {
                name: "gradient_tabu_tenure".into(),
                label: "Tabu Tenure".into(),
                description: "Steps an edge stays tabu during descent/repair".into(),
                param_type: ParamType::Int { min: 5, max: 100 },
                default: serde_json::json!(25),
                adjustable: true,
            },
            ConfigParam {
                name: "gradient_perturb_flips".into(),
                label: "Perturb Flips".into(),
                description: "Random flips between trials to escape basins".into(),
                param_type: ParamType::Int { min: 0, max: 50 },
                default: serde_json::json!(5),
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
            ConfigParam {
                name: "target_k".into(),
                label: "Target K".into(),
                description: "Clique size to minimize in graph (red)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
            ConfigParam {
                name: "target_ell".into(),
                label: "Target Ell".into(),
                description: "Clique size to minimize in complement (blue)".into(),
                param_type: ParamType::Int { min: 3, max: 10 },
                default: serde_json::json!(5),
                adjustable: false,
            },
        ]
    }

    fn search(&self, job: &SearchJob, observer: &dyn SearchObserver) -> SearchResult {
        let descent_steps = job
            .config
            .get("gradient_descent_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as u32;
        let repair_steps = job
            .config
            .get("gradient_repair_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(5000) as u32;
        let tabu_tenure = job
            .config
            .get("gradient_tabu_tenure")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as u32;
        let perturb_flips = job
            .config
            .get("gradient_perturb_flips")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32;
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

        let n = job.n;
        let max_iters = job.max_iters;
        let edge_count = (n * (n - 1) / 2) as usize;
        let mut rng = SmallRng::seed_from_u64(job.seed);

        let all_edges: Vec<(u32, u32)> = {
            let mut v = Vec::with_capacity(edge_count);
            for i in 0..n {
                for j in (i + 1)..n {
                    v.push((i, j));
                }
            }
            v
        };

        let seed_graph = job
            .init_graph
            .clone()
            .unwrap_or_else(|| crate::init::random_graph(n, &mut rng));

        let mut known_cids = job.known_cids.clone();
        let mut discovery_count: u64 = 0;
        let mut best_valid: Option<AdjacencyMatrix> = None;
        let mut best_valid_score: Option<(u64, u64)> = None;
        let mut total_iters: u64 = 0;
        let mut trials: u64 = 0;
        let mut repairs_succeeded: u64 = 0;
        let max_polish_per_round: u32 = 10;
        let mut polish_calls: u32 = 0;

        observer.on_progress(&ProgressInfo {
            graph: seed_graph.clone(),
            n,
            strategy: "gradient".to_string(),
            iteration: 0,
            max_iters,
            valid: false,
            violation_score: 0,
            discoveries_so_far: 0,
        });

        while total_iters < max_iters && !observer.is_cancelled() {
            trials += 1;

            // Initialize from seed with random perturbation
            let mut graph = seed_graph.clone();
            if trials > 1 && perturb_flips > 0 {
                for _ in 0..perturb_flips {
                    let &(u, v) = &all_edges[rng.gen_range(0..all_edges.len())];
                    let cur = graph.edge(u, v);
                    graph.set_edge(u, v, !cur);
                }
            }

            let mut comp = graph.complement();
            let mut adj_nbrs = NeighborSet::from_adj(&graph);
            let mut comp_nbrs = NeighborSet::from_adj(&comp);

            let mut red_4 = count_cliques(&adj_nbrs, 4, n);
            let mut blue_4 = count_cliques(&comp_nbrs, 4, n);

            let mut tabu_until: Vec<u32> = vec![0; edge_count];

            // ========== Phase 1: Greedy 4c Descent ==========
            let initial_max_4c = red_4.max(blue_4);
            let mut phase1_steps: u32 = 0;

            for step in 1..=descent_steps {
                if total_iters >= max_iters || observer.is_cancelled() {
                    break;
                }
                total_iters += 1;

                // Periodic recount to correct delta drift
                if step % 100 == 0 {
                    red_4 = count_cliques(&adj_nbrs, 4, n);
                    blue_4 = count_cliques(&comp_nbrs, 4, n);
                }

                // Find edge that most reduces the 4c tier (max, min)
                let current_tier = (red_4.max(blue_4), red_4.min(blue_4));
                let mut best_edge: Option<(u32, u32, i64, i64)> = None;
                let mut best_tier = (i64::MAX, i64::MAX);

                for &(u, v) in &all_edges {
                    let eidx = edge_index(u, v, n);
                    if tabu_until[eidx] > step {
                        continue;
                    }

                    let (d_r4, d_b4) = violation_delta(&adj_nbrs, &comp_nbrs, 4, 4, u, v);
                    let new_r4 = (red_4 as i64 + d_r4).max(0);
                    let new_b4 = (blue_4 as i64 + d_b4).max(0);
                    let new_tier = (new_r4.max(new_b4), new_r4.min(new_b4));

                    if new_tier < best_tier {
                        best_tier = new_tier;
                        best_edge = Some((u, v, d_r4, d_b4));
                    }
                }

                if let Some((u, v, d_r4, d_b4)) = best_edge {
                    let cur = graph.edge(u, v);
                    graph.set_edge(u, v, !cur);
                    comp.set_edge(u, v, cur);
                    adj_nbrs.flip_edge(u, v);
                    comp_nbrs.flip_edge(u, v);
                    red_4 = (red_4 as i64 + d_r4).max(0) as u64;
                    blue_4 = (blue_4 as i64 + d_b4).max(0) as u64;
                    tabu_until[edge_index(u, v, n)] = step + tabu_tenure;
                    phase1_steps = step;

                    // Early exit if tier stopped improving for tabu_tenure steps
                    let new_tier_u64 = (red_4.max(blue_4), red_4.min(blue_4));
                    if new_tier_u64 >= current_tier && step > tabu_tenure * 2 {
                        break;
                    }
                } else {
                    break;
                }
            }

            let post_descent_max_4c = red_4.max(blue_4);

            // ========== Phase 2: Violation Repair ==========
            let mut kc = count_cliques(&adj_nbrs, k, n);
            let mut ei = count_cliques(&comp_nbrs, ell, n);
            let mut violations = kc + ei;
            let initial_violations = violations;

            // Reset tabu for Phase 2
            tabu_until.fill(0);

            let mut phase2_steps: u32 = 0;

            if violations > 0 {
                for step in 1..=repair_steps {
                    if total_iters >= max_iters || observer.is_cancelled() {
                        break;
                    }
                    total_iters += 1;

                    if violations == 0 {
                        break;
                    }

                    // Periodic recount
                    if step % 200 == 0 {
                        kc = count_cliques(&adj_nbrs, k, n);
                        ei = count_cliques(&comp_nbrs, ell, n);
                        violations = kc + ei;
                        if violations == 0 {
                            break;
                        }
                    }

                    // Find edge that most reduces violations
                    let mut best_edge: Option<(u32, u32, i64, i64)> = None;
                    let mut best_delta: i64 = i64::MAX;

                    for &(u, v) in &all_edges {
                        let eidx = edge_index(u, v, n);
                        if tabu_until[eidx] > step {
                            continue;
                        }

                        let (dk, de) = violation_delta(&adj_nbrs, &comp_nbrs, k, ell, u, v);
                        let total_delta = dk + de;

                        if total_delta < best_delta {
                            best_delta = total_delta;
                            best_edge = Some((u, v, dk, de));
                        }
                    }

                    if let Some((u, v, dk, de)) = best_edge {
                        let cur = graph.edge(u, v);
                        graph.set_edge(u, v, !cur);
                        comp.set_edge(u, v, cur);
                        adj_nbrs.flip_edge(u, v);
                        comp_nbrs.flip_edge(u, v);
                        kc = (kc as i64 + dk).max(0) as u64;
                        ei = (ei as i64 + de).max(0) as u64;
                        violations = kc + ei;
                        tabu_until[edge_index(u, v, n)] = step + tabu_tenure;
                        phase2_steps = step;
                    } else {
                        break;
                    }
                }
            }

            // Full recount to correct drift
            kc = count_cliques(&adj_nbrs, k, n);
            ei = count_cliques(&comp_nbrs, ell, n);
            violations = kc + ei;

            // ========== Post-repair: Score and Polish ==========
            if violations == 0 {
                repairs_succeeded += 1;
                red_4 = count_cliques(&adj_nbrs, 4, n);
                blue_4 = count_cliques(&comp_nbrs, 4, n);

                let max_4c = red_4.max(blue_4);
                let min_4c = red_4.min(blue_4);
                let is_new_best = match best_valid_score {
                    Some((bmax, bmin)) => (max_4c, min_4c) < (bmax, bmin),
                    None => true,
                };

                let (canonical, _) = canonical_form(&graph);
                let cid = extremal_graph::compute_cid(&canonical);
                if known_cids.insert(cid) {
                    observer.on_discovery(&RawDiscovery {
                        graph: graph.clone(),
                        iteration: total_iters,
                    });
                    discovery_count += 1;

                    if is_new_best {
                        best_valid = Some(graph.clone());
                        best_valid_score = Some((max_4c, min_4c));
                    }
                }

                // Polish valid graph
                if polish_max_steps > 0 && polish_calls < max_polish_per_round {
                    polish_calls += 1;
                    if let Some(polished) = crate::polish::polish_valid_graph(
                        &graph,
                        k,
                        ell,
                        polish_max_steps,
                        polish_tabu_tenure,
                        false,
                        &mut known_cids,
                        observer,
                        total_iters,
                    ) {
                        let p_adj = NeighborSet::from_adj(&polished);
                        let p_comp_g = polished.complement();
                        let p_comp = NeighborSet::from_adj(&p_comp_g);
                        let p_r4 = count_cliques(&p_adj, 4, polished.n());
                        let p_b4 = count_cliques(&p_comp, 4, polished.n());
                        let p_max = p_r4.max(p_b4);
                        let p_min = p_r4.min(p_b4);
                        let polished_better = match best_valid_score {
                            Some((bmax, bmin)) => (p_max, p_min) < (bmax, bmin),
                            None => true,
                        };
                        if polished_better {
                            best_valid = Some(polished);
                            best_valid_score = Some((p_max, p_min));
                        }
                    }
                }

                debug!(
                    trial = trials,
                    phase1_steps,
                    initial_max_4c,
                    post_descent_max_4c,
                    phase2_steps,
                    initial_violations,
                    result_4c = format!("({},{})", max_4c, min_4c),
                    "gradient: repair succeeded"
                );
            } else {
                debug!(
                    trial = trials,
                    phase1_steps,
                    initial_max_4c,
                    post_descent_max_4c,
                    phase2_steps,
                    initial_violations,
                    remaining_violations = violations,
                    "gradient: repair failed"
                );
            }

            // Periodic progress
            if trials.is_multiple_of(10) {
                observer.on_progress(&ProgressInfo {
                    graph: best_valid.as_ref().unwrap_or(&seed_graph).clone(),
                    n,
                    strategy: "gradient".to_string(),
                    iteration: total_iters,
                    max_iters,
                    valid: best_valid.is_some(),
                    violation_score: if best_valid.is_some() {
                        0
                    } else {
                        violations as u32
                    },
                    discoveries_so_far: discovery_count,
                });
            }
        }

        debug!(
            trials,
            repairs_succeeded,
            discoveries = discovery_count,
            polish_calls,
            best_4c = best_valid_score.map(|(m, _)| m),
            "gradient: round complete"
        );

        SearchResult {
            valid: best_valid.is_some(),
            best_graph: best_valid.or(Some(seed_graph)),
            iterations_used: total_iters,
            discoveries: Vec::new(),
            carry_state: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::paley_graph;
    use extremal_worker_api::{CollectingObserver, NoOpObserver};
    use std::collections::HashSet;

    fn make_job(n: u32, k: u32, ell: u32, max_iters: u64) -> SearchJob {
        SearchJob {
            n,
            max_iters,
            seed: 42,
            init_graph: None,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "gradient_descent_steps": 100,
                "gradient_repair_steps": 5000,
                "gradient_tabu_tenure": 15,
                "gradient_perturb_flips": 3,
                "polish_max_steps": 50,
                "polish_tabu_tenure": 15,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 10_000,
            carry_state: None,
        }
    }

    #[test]
    fn gradient_finds_r33_n5() {
        let mut job = make_job(5, 3, 3, 100_000);
        job.init_graph = Some(paley_graph(5));
        let result = GradientDescent.search(&job, &NoOpObserver);
        assert!(
            result.valid,
            "gradient should find valid R(3,3) on 5 vertices"
        );
    }

    #[test]
    fn gradient_finds_r44_n17() {
        let mut job = make_job(17, 4, 4, 500_000);
        job.init_graph = Some(paley_graph(17));
        let result = GradientDescent.search(&job, &NoOpObserver);
        assert!(
            result.valid,
            "gradient should find valid R(4,4) on 17 vertices"
        );
    }

    #[test]
    fn gradient_reports_discoveries() {
        let mut job = make_job(5, 3, 3, 100_000);
        job.init_graph = Some(paley_graph(5));
        let observer = CollectingObserver::new();
        let result = GradientDescent.search(&job, &observer);
        assert!(result.valid);
        let discoveries = observer.drain();
        assert!(
            !discoveries.is_empty(),
            "gradient should discover at least 1 valid graph"
        );
        // All discoveries must be valid
        for d in &discoveries {
            let adj = NeighborSet::from_adj(&d.graph);
            let comp = NeighborSet::from_adj(&d.graph.complement());
            assert_eq!(
                count_cliques(&adj, 3, 5) + count_cliques(&comp, 3, 5),
                0,
                "discovered graph must be valid R(3,3)"
            );
        }
    }

    #[test]
    fn gradient_multiple_trials() {
        // Verify gradient runs multiple trials with perturbation and finds valid graphs
        let mut job = make_job(17, 4, 4, 500_000);
        job.init_graph = Some(paley_graph(17));
        job.config = serde_json::json!({
            "target_k": 4,
            "target_ell": 4,
            "gradient_descent_steps": 50,
            "gradient_repair_steps": 2000,
            "gradient_tabu_tenure": 15,
            "gradient_perturb_flips": 5,
            "polish_max_steps": 0,
        });
        let observer = CollectingObserver::new();
        let result = GradientDescent.search(&job, &observer);
        assert!(result.valid, "gradient should find valid R(4,4) graphs");
        let discoveries = observer.drain();
        // All discoveries must be valid
        for d in &discoveries {
            let adj = NeighborSet::from_adj(&d.graph);
            let comp = NeighborSet::from_adj(&d.graph.complement());
            assert_eq!(
                count_cliques(&adj, 4, 17) + count_cliques(&comp, 4, 17),
                0,
                "all gradient discoveries must be valid R(4,4)"
            );
        }
    }
}
