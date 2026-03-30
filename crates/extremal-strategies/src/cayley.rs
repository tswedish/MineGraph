//! Cayley graph enumeration over Z_p × Z_p for Ramsey graph search.
//!
//! For n = p² (prime p), exhaustively enumerates all Cayley graphs of the
//! elementary abelian group Z_p × Z_p. This explores a fundamentally different
//! algebraic subspace from circulant graphs (which cover the cyclic group Z_n).
//!
//! ## Key insight
//!
//! The Paley graph P(25) is a Cayley graph of GF(25) ≅ Z_5 × Z_5 (additive
//! group) over the set of quadratic residues. So for n=25 this search space
//! **contains the Paley graph** plus 4095 other algebraically structured
//! alternatives — many with |Aut| ≥ 25.
//!
//! ## Algorithm
//!
//! 1. Factor n = p² and enumerate inverse pairs in Z_p × Z_p \ {(0,0)}.
//! 2. Enumerate all 2^(num_pairs) connection sets (4096 for n=25).
//! 3. Build Cayley graph for each set, check R(k,ℓ) validity.
//! 4. Polish and report any valid graphs.
//! 5. Subsequent rounds: skip (exhaustive search already complete).
//!
//! ## Search space size
//!
//! | n   | p | pairs | graphs |
//! |-----|---|-------|--------|
//! | 4   | 2 | 3     | 8      |
//! | 9   | 3 | 4     | 16     |
//! | 25  | 5 | 12    | 4096   |
//! | 49  | 7 | 24    | 16M    |

use extremal_graph::AdjacencyMatrix;
use extremal_scoring::automorphism::canonical_form;
use extremal_scoring::clique::{NeighborSet, count_cliques};
use extremal_worker_api::{
    ConfigParam, ParamType, ProgressInfo, RawDiscovery, SearchJob, SearchObserver, SearchResult,
    SearchStrategy,
};
use tracing::{debug, info, warn};

pub struct CayleySearch;

/// Check if n = p² for a small prime p. Returns p if so.
fn prime_sqrt(n: u32) -> Option<u32> {
    [2u32, 3, 5, 7].into_iter().find(|&p| p * p == n)
}

/// Enumerate independent inverse pairs in Z_p × Z_p \ {(0,0)}.
///
/// For each non-identity element (a,b), its inverse is (-a mod p, -b mod p).
/// We pick the lexicographically smaller representative of each pair.
/// Including a pair in the connection set S means adding both (a,b) and (-a,-b).
fn enumerate_pairs(p: u32) -> Vec<(u32, u32)> {
    let mut seen = vec![false; (p * p) as usize];
    let mut pairs = Vec::new();

    for a in 0..p {
        for b in 0..p {
            if a == 0 && b == 0 {
                continue;
            }
            let idx = (a * p + b) as usize;
            if seen[idx] {
                continue;
            }

            let inv_a = if a == 0 { 0 } else { p - a };
            let inv_b = if b == 0 { 0 } else { p - b };
            let inv_idx = (inv_a * p + inv_b) as usize;

            seen[idx] = true;
            seen[inv_idx] = true;
            pairs.push((a, b));
        }
    }

    pairs
}

/// Build a Cayley graph of Z_p × Z_p with connection set defined by `mask`.
///
/// Vertex (i,j) maps to index p*i + j. Edge between u=(i,j) and v=(i',j')
/// iff ((i'-i) mod p, (j'-j) mod p) ∈ S.
fn build_cayley(p: u32, pairs: &[(u32, u32)], mask: u32) -> AdjacencyMatrix {
    let n = p * p;
    let mut g = AdjacencyMatrix::new(n);

    // Build connection set from mask (each selected pair contributes both elements)
    let mut conn_set = Vec::new();
    for (bit, &(a, b)) in pairs.iter().enumerate() {
        if mask & (1 << bit) != 0 {
            conn_set.push((a, b));
            let inv_a = if a == 0 { 0 } else { p - a };
            let inv_b = if b == 0 { 0 } else { p - b };
            if (inv_a, inv_b) != (a, b) {
                conn_set.push((inv_a, inv_b));
            }
        }
    }

    // Add edges: vertex u=(i,j) connects to u+(da,db) for each (da,db) in S
    for i in 0..p {
        for j in 0..p {
            let u = i * p + j;
            for &(da, db) in &conn_set {
                let ni = (i + da) % p;
                let nj = (j + db) % p;
                let v = ni * p + nj;
                if u < v {
                    g.set_edge(u, v, true);
                }
            }
        }
    }

    g
}

/// Maximum number of pairs we'll exhaustively enumerate (2^20 = ~1M graphs).
const MAX_EXHAUSTIVE_PAIRS: u32 = 20;

impl SearchStrategy for CayleySearch {
    fn id(&self) -> &str {
        "cayley"
    }

    fn name(&self) -> &str {
        "Cayley Z_p×Z_p Enumeration"
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

        // Check if n = p² for some prime p
        let p = match prime_sqrt(n) {
            Some(p) => p,
            None => {
                warn!(n, "cayley: n is not p² for any small prime, skipping");
                return SearchResult {
                    valid: false,
                    best_graph: None,
                    iterations_used: 0,
                    discoveries: Vec::new(),
                    carry_state: Some(Box::new(true)),
                };
            }
        };

        // Check carry_state — skip if already enumerated
        if let Some(ref state) = job.carry_state
            && state.downcast_ref::<bool>().copied().unwrap_or(false)
        {
            debug!("cayley: exhaustive enumeration already complete, skipping");
            return SearchResult {
                valid: false,
                best_graph: None,
                iterations_used: 0,
                discoveries: Vec::new(),
                carry_state: Some(Box::new(true)),
            };
        }

        let pairs = enumerate_pairs(p);
        let num_pairs = pairs.len() as u32;

        if num_pairs > MAX_EXHAUSTIVE_PAIRS {
            warn!(
                n,
                p, num_pairs, "cayley: too many pairs for exhaustive search, skipping"
            );
            return SearchResult {
                valid: false,
                best_graph: None,
                iterations_used: 0,
                discoveries: Vec::new(),
                carry_state: Some(Box::new(true)),
            };
        }

        let total_sets = 1u32 << num_pairs;

        info!(
            n,
            p, num_pairs, total_sets, "cayley: beginning Z_{p}×Z_{p} exhaustive enumeration"
        );

        let mut known_cids = job.known_cids.clone();
        let mut discovery_count: u64 = 0;
        let mut valid_count: u64 = 0;
        let mut best_valid: Option<AdjacencyMatrix> = None;
        let mut best_valid_score: Option<(u64, u64)> = None;
        let mut polish_calls: u32 = 0;
        let max_polish_per_round: u32 = 50;

        observer.on_progress(&ProgressInfo {
            graph: AdjacencyMatrix::new(n),
            n,
            strategy: "cayley".to_string(),
            iteration: 0,
            max_iters: total_sets as u64,
            valid: false,
            violation_score: 0,
            discoveries_so_far: 0,
        });

        for mask in 0..total_sets {
            if observer.is_cancelled() {
                break;
            }

            // Build Cayley graph of Z_p × Z_p with this connection set
            let adj = build_cayley(p, &pairs, mask);
            let adj_nbrs = NeighborSet::from_adj(&adj);
            let comp = adj.complement();
            let comp_nbrs = NeighborSet::from_adj(&comp);

            // Check R(k,ℓ) validity
            let kc = count_cliques(&adj_nbrs, k, n);
            let ei = count_cliques(&comp_nbrs, ell, n);

            if kc + ei == 0 {
                valid_count += 1;

                let red_4 = count_cliques(&adj_nbrs, 4, n);
                let blue_4 = count_cliques(&comp_nbrs, 4, n);
                let max_4c = red_4.max(blue_4);
                let min_4c = red_4.min(blue_4);

                // Canonicalize for CID dedup
                let (canonical, _) = canonical_form(&adj);
                let cid = extremal_graph::compute_cid(&canonical);

                if known_cids.insert(cid) {
                    observer.on_discovery(&RawDiscovery {
                        graph: adj.clone(),
                        iteration: mask as u64,
                    });
                    discovery_count += 1;

                    let is_better = match best_valid_score {
                        Some((bmax, bmin)) => (max_4c, min_4c) < (bmax, bmin),
                        None => true,
                    };
                    if is_better {
                        best_valid = Some(adj.clone());
                        best_valid_score = Some((max_4c, min_4c));
                        info!(
                            mask,
                            red_4, blue_4, max_4c, min_4c, "cayley: new best valid graph"
                        );
                    }

                    // Polish for score improvement
                    if polish_max_steps > 0 && polish_calls < max_polish_per_round {
                        polish_calls += 1;
                        if let Some(polished) = crate::polish::polish_valid_graph(
                            &adj,
                            k,
                            ell,
                            polish_max_steps,
                            polish_tabu_tenure,
                            false,
                            &mut known_cids,
                            observer,
                            mask as u64,
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
                }

                debug!(
                    mask,
                    red_4, blue_4, valid_count, "cayley: valid R({k},{ell}) graph found"
                );
            }

            // Periodic progress
            if mask % 256 == 0 || mask == total_sets - 1 {
                observer.on_progress(&ProgressInfo {
                    graph: best_valid.clone().unwrap_or_else(|| adj.clone()),
                    n,
                    strategy: "cayley".to_string(),
                    iteration: mask as u64,
                    max_iters: total_sets as u64,
                    valid: best_valid.is_some(),
                    violation_score: (kc + ei) as u32,
                    discoveries_so_far: discovery_count,
                });
            }
        }

        info!(
            valid_count,
            discovery_count,
            polish_calls,
            best_4c = ?best_valid_score.map(|(m, _)| m),
            "cayley: enumeration complete"
        );

        SearchResult {
            valid: best_valid.is_some(),
            best_graph: best_valid,
            iterations_used: total_sets as u64,
            discoveries: Vec::new(),
            carry_state: Some(Box::new(true)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use extremal_worker_api::{CollectingObserver, NoOpObserver};
    use std::collections::HashSet;

    fn make_job(n: u32, k: u32, ell: u32) -> SearchJob {
        SearchJob {
            n,
            max_iters: 1_000_000,
            seed: 42,
            init_graph: None,
            config: serde_json::json!({
                "target_k": k,
                "target_ell": ell,
                "polish_max_steps": 0,
            }),
            known_cids: HashSet::new(),
            max_known_cids: 10_000,
            carry_state: None,
        }
    }

    #[test]
    fn prime_sqrt_works() {
        assert_eq!(prime_sqrt(4), Some(2));
        assert_eq!(prime_sqrt(9), Some(3));
        assert_eq!(prime_sqrt(25), Some(5));
        assert_eq!(prime_sqrt(49), Some(7));
        assert_eq!(prime_sqrt(16), None); // 4² but 4 is not prime
        assert_eq!(prime_sqrt(5), None);
        assert_eq!(prime_sqrt(17), None);
    }

    #[test]
    fn enumerate_pairs_z2xz2() {
        // Z_2×Z_2: 3 non-identity elements, all self-inverse
        let pairs = enumerate_pairs(2);
        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn enumerate_pairs_z3xz3() {
        // Z_3×Z_3: 8 non-identity, 4 inverse pairs
        let pairs = enumerate_pairs(3);
        assert_eq!(pairs.len(), 4);
    }

    #[test]
    fn enumerate_pairs_z5xz5() {
        // Z_5×Z_5: 24 non-identity, 12 inverse pairs
        let pairs = enumerate_pairs(5);
        assert_eq!(pairs.len(), 12);
    }

    #[test]
    fn build_cayley_empty() {
        let pairs = enumerate_pairs(3);
        let g = build_cayley(3, &pairs, 0);
        assert_eq!(g.num_edges(), 0);
    }

    #[test]
    fn build_cayley_vertex_transitive() {
        // Any Cayley graph should be vertex-transitive (all degrees equal)
        let pairs = enumerate_pairs(3);
        let g = build_cayley(3, &pairs, 0b1010); // arbitrary mask
        let d0 = g.degree(0);
        for v in 1..9 {
            assert_eq!(
                g.degree(v),
                d0,
                "Cayley graph must be vertex-transitive: deg({v}) != deg(0)"
            );
        }
    }

    #[test]
    fn cayley_finds_r33_n4() {
        // Z_2×Z_2 on 4 vertices. R(3,3)=6 > 4, valid colorings exist.
        let job = make_job(4, 3, 3);
        let observer = CollectingObserver::new();
        let result = CayleySearch.search(&job, &observer);
        assert!(
            result.valid,
            "should find valid R(3,3) Cayley on 4 vertices"
        );
        let discoveries = observer.drain();
        assert!(
            !discoveries.is_empty(),
            "should discover at least 1 valid Cayley graph"
        );
    }

    #[test]
    fn cayley_finds_r44_n9() {
        // Z_3×Z_3 on 9 vertices. Paley(9) is a Cayley graph of GF(9)≅Z_3×Z_3
        // and is R(4,4)-valid since R(4,4)=18 > 9.
        let job = make_job(9, 4, 4);
        let result = CayleySearch.search(&job, &NoOpObserver);
        assert!(
            result.valid,
            "should find valid R(4,4) Cayley on 9 vertices"
        );
    }

    #[test]
    fn cayley_skips_non_square_prime() {
        // n=17 is prime, not p²
        let job = make_job(17, 4, 4);
        let result = CayleySearch.search(&job, &NoOpObserver);
        assert_eq!(result.iterations_used, 0, "should skip non-p² values");
    }

    #[test]
    fn cayley_skips_second_round() {
        let mut job = make_job(4, 3, 3);
        job.carry_state = Some(Box::new(true));
        let result = CayleySearch.search(&job, &NoOpObserver);
        assert_eq!(result.iterations_used, 0, "should skip when already done");
    }

    #[test]
    fn cayley_reports_carry_state() {
        let job = make_job(4, 3, 3);
        let result = CayleySearch.search(&job, &NoOpObserver);
        let state = result.carry_state.expect("should return carry_state");
        assert_eq!(state.downcast_ref::<bool>(), Some(&true));
    }
}
