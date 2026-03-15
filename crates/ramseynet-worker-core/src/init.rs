//! Graph initialization for the worker platform.
//!
//! The platform provides seed graphs to strategies via SearchJob.init_graph.
//! This module contains the graph construction functions used by the engine.

use ramseynet_graph::AdjacencyMatrix;
use rand::rngs::SmallRng;
use rand::Rng;

/// How the engine produces seed graphs for each search round.
#[derive(Clone, Debug)]
pub enum InitMode {
    /// Paley graph (deterministic algebraic construction).
    Paley,
    /// Perturbed Paley with 5% random edge noise.
    PerturbedPaley,
    /// Uniform random G(n, 0.5).
    Random,
    /// Seed from server leaderboard each round.
    Leaderboard,
}

/// Generate a seed graph using the init mode (when the local/server pool is empty).
pub fn make_init_graph(mode: &InitMode, n: u32, rng: &mut SmallRng) -> AdjacencyMatrix {
    match mode {
        InitMode::Paley => paley_graph(n),
        InitMode::PerturbedPaley => {
            let mut g = paley_graph(n);
            perturb(&mut g, 0.05, rng);
            g
        }
        InitMode::Random => random_graph(n, rng),
        InitMode::Leaderboard => {
            // Fallback when leaderboard pool is empty
            let mut g = paley_graph(n);
            perturb(&mut g, 0.05, rng);
            g
        }
    }
}

/// Sample a seed graph from a pool, with bias and noise.
///
/// If the pool is empty, falls back to the init mode construction.
pub fn sample_init_graph(
    pool: &[AdjacencyMatrix],
    sample_bias: f64,
    n: u32,
    noise_flips: u32,
    rng: &mut SmallRng,
) -> AdjacencyMatrix {
    if !pool.is_empty() {
        let idx = weighted_sample(pool.len(), sample_bias, rng);
        let mut g = pool[idx].clone();
        if noise_flips > 0 {
            flip_random_edges(&mut g, noise_flips, rng);
        }
        g
    } else {
        // Fallback to perturbed Paley
        let mut g = paley_graph(n);
        perturb(&mut g, 0.05, rng);
        g
    }
}

/// Select an index from 0..len using exponential weighting.
fn weighted_sample(len: usize, bias: f64, rng: &mut SmallRng) -> usize {
    if len <= 1 || bias <= 0.0 {
        return rng.gen_range(0..len);
    }
    let n = len as f64;
    let mut cumulative = Vec::with_capacity(len);
    let mut total = 0.0_f64;
    for i in 0..len {
        let w = (-bias * 5.0 * i as f64 / (n - 1.0)).exp();
        total += w;
        cumulative.push(total);
    }
    let r = rng.gen::<f64>() * total;
    cumulative.iter().position(|&c| c > r).unwrap_or(len - 1)
}

/// Flip exactly `count` randomly chosen edges.
fn flip_random_edges(graph: &mut AdjacencyMatrix, count: u32, rng: &mut SmallRng) {
    let n = graph.n();
    let num_possible = n * (n - 1) / 2;
    if num_possible == 0 {
        return;
    }
    for _ in 0..count {
        let idx = rng.gen_range(0..num_possible);
        let (i, j) = linear_to_edge(idx, n);
        let current = graph.edge(i, j);
        graph.set_edge(i, j, !current);
    }
}

fn linear_to_edge(idx: u32, n: u32) -> (u32, u32) {
    let mut remaining = idx;
    for i in 0..n {
        let row_len = n - 1 - i;
        if remaining < row_len {
            return (i, i + 1 + remaining);
        }
        remaining -= row_len;
    }
    unreachable!()
}

fn random_graph(n: u32, rng: &mut SmallRng) -> AdjacencyMatrix {
    let mut g = AdjacencyMatrix::new(n);
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.gen_bool(0.5) {
                g.set_edge(i, j, true);
            }
        }
    }
    g
}

fn perturb(graph: &mut AdjacencyMatrix, flip_fraction: f64, rng: &mut SmallRng) {
    let n = graph.n();
    for i in 0..n {
        for j in (i + 1)..n {
            if rng.gen_bool(flip_fraction) {
                let current = graph.edge(i, j);
                graph.set_edge(i, j, !current);
            }
        }
    }
}

/// Construct a Paley graph or Paley-induced subgraph on n vertices.
pub fn paley_graph(n: u32) -> AdjacencyMatrix {
    let p = smallest_paley_prime(n);
    let qr = quadratic_residues(p);

    let mut g = AdjacencyMatrix::new(n);
    for i in 0..n {
        for j in (i + 1)..n {
            let diff = ((i as i64 - j as i64).rem_euclid(p as i64)) as u32;
            if qr.contains(&diff) {
                g.set_edge(i, j, true);
            }
        }
    }
    g
}

fn smallest_paley_prime(n: u32) -> u32 {
    let mut p = n.max(5);
    loop {
        if p % 4 == 1 && is_prime(p) {
            return p;
        }
        p += 1;
    }
}

fn is_prime(n: u32) -> bool {
    if n < 2 {
        return false;
    }
    if n < 4 {
        return true;
    }
    if n.is_multiple_of(2) || n.is_multiple_of(3) {
        return false;
    }
    let mut i = 5u32;
    while i * i <= n {
        if n.is_multiple_of(i) || n.is_multiple_of(i + 2) {
            return false;
        }
        i += 6;
    }
    true
}

fn quadratic_residues(p: u32) -> Vec<u32> {
    let mut qr = vec![false; p as usize];
    for x in 1..p {
        let r = ((x as u64 * x as u64) % p as u64) as u32;
        qr[r as usize] = true;
    }
    (1..p).filter(|&i| qr[i as usize]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn paley_17_constructs() {
        let g = paley_graph(17);
        assert_eq!(g.n(), 17);
    }

    #[test]
    fn paley_5_constructs() {
        let g = paley_graph(5);
        assert_eq!(g.n(), 5);
    }

    #[test]
    fn sample_from_pool() {
        let mut rng = SmallRng::seed_from_u64(42);
        let c5 = paley_graph(5);
        let pool = vec![c5.clone()];
        let g = sample_init_graph(&pool, 0.0, 5, 0, &mut rng);
        assert_eq!(g.num_edges(), c5.num_edges());
    }

    #[test]
    fn sample_with_noise() {
        let mut rng = SmallRng::seed_from_u64(42);
        let c5 = paley_graph(5);
        let pool = vec![c5.clone()];
        let g = sample_init_graph(&pool, 0.0, 5, 3, &mut rng);
        let mut diffs = 0;
        for i in 0..5 {
            for j in (i + 1)..5 {
                if g.edge(i, j) != c5.edge(i, j) {
                    diffs += 1;
                }
            }
        }
        assert!(diffs > 0 && diffs <= 3, "expected 1-3 diffs, got {diffs}");
    }

    #[test]
    fn sample_fallback_when_empty() {
        let mut rng = SmallRng::seed_from_u64(42);
        let g = sample_init_graph(&[], 0.5, 5, 0, &mut rng);
        assert_eq!(g.n(), 5);
    }
}
