use rand::rngs::SmallRng;
use ramseynet_graph::AdjacencyMatrix;

/// Result of a search attempt.
#[derive(Clone, Debug)]
pub struct SearchResult {
    /// The best graph found (may or may not be valid).
    pub graph: AdjacencyMatrix,
    /// Whether the graph is Ramsey-valid.
    pub valid: bool,
    /// Number of iterations performed.
    pub iterations: u64,
}

/// Trait for Ramsey graph search heuristics.
pub trait Searcher: Send + Sync + 'static {
    /// Search for a Ramsey(k, ell)-valid graph on n vertices.
    fn search(&self, n: u32, k: u32, ell: u32, max_iters: u64, rng: &mut SmallRng) -> SearchResult;

    /// Human-readable name for this strategy.
    fn name(&self) -> &'static str;
}
