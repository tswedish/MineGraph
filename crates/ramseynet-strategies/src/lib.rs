//! Search strategy implementations for RamseyNet.
//!
//! Currently provides tree/beam search as the sole strategy.
//! Additional strategies can be added by implementing the
//! [`SearchStrategy`] trait from `ramseynet-worker-api`.

pub mod tree;

use ramseynet_worker_api::SearchStrategy;

/// Return all available search strategies.
pub fn default_strategies() -> Vec<Box<dyn SearchStrategy>> {
    vec![Box::new(tree::TreeSearch)]
}
