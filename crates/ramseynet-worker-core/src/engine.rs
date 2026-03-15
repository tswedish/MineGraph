//! Worker engine: main orchestration loop.
//!
//! Coordinates search strategies, leaderboard sync, and the submission
//! pipeline. Supports both online (server-connected) and offline modes.
//! Maintains a local discovery pool that feeds back as seed graphs for
//! subsequent search rounds (self-learning convergence).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ramseynet_graph::{compute_cid, rgxf, AdjacencyMatrix};
use ramseynet_types::GraphCid;
use ramseynet_verifier::scoring::{compute_score_canonical, GraphScore};
use ramseynet_worker_api::{ProgressInfo, SearchJob, SearchObserver, SearchStrategy};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::client::{ServerClient, ThresholdResponse};
use crate::error::WorkerError;
use crate::init::{self, InitMode};
use crate::VizBridge;

/// Configuration for the worker engine.
pub struct EngineConfig {
    pub k: u32,
    pub ell: u32,
    pub n: u32,
    pub max_iters: u64,
    pub no_backoff: bool,
    pub offline: bool,
    pub sample_bias: f64,
    pub leaderboard_sample_size: u32,
    pub collector_capacity: usize,
    pub max_known_cids: usize,
    pub noise_flips: u32,
    pub init_mode: InitMode,
    /// Strategy-specific config passed to SearchJob.
    pub strategy_config: serde_json::Value,
}

/// Cached admission threshold from the server.
struct AdmissionThreshold {
    worst_score: Option<GraphScore>,
}

impl AdmissionThreshold {
    fn open() -> Self {
        Self { worst_score: None }
    }

    fn from_response(resp: &ThresholdResponse) -> Self {
        let worst_score = if resp.entry_count >= resp.capacity {
            match (
                resp.worst_tier1_max,
                resp.worst_tier1_min,
                resp.worst_goodman_gap,
                resp.worst_tier2_aut,
                resp.worst_tier3_cid.as_ref(),
            ) {
                (Some(t1_max), Some(t1_min), Some(goodman_gap), Some(t2_aut), Some(t3_cid)) => {
                    match GraphCid::from_hex(t3_cid) {
                        Ok(cid) => Some(GraphScore::new(
                            0, 0, 0, t1_max, t1_min, goodman_gap, 0, t2_aut, cid,
                        )),
                        Err(_) => None,
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        Self { worst_score }
    }

    fn would_admit(&self, score: &GraphScore) -> bool {
        match &self.worst_score {
            None => true,
            Some(worst) => score < worst,
        }
    }
}

/// Known CID set for cross-round deduplication.
#[derive(Clone, Default)]
pub struct KnownCids {
    inner: std::collections::HashSet<GraphCid>,
}

impl KnownCids {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_from_hex(&mut self, cids: &[String]) {
        for hex in cids {
            if let Ok(cid) = GraphCid::from_hex(hex) {
                self.inner.insert(cid);
            }
        }
    }

    pub fn insert(&mut self, cid: GraphCid) {
        self.inner.insert(cid);
    }

    pub fn insert_hex(&mut self, hex: &str) {
        if let Ok(cid) = GraphCid::from_hex(hex) {
            self.inner.insert(cid);
        }
    }

    pub fn contains(&self, cid: &GraphCid) -> bool {
        self.inner.contains(cid)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn snapshot_trimmed(&self, max: usize) -> std::collections::HashSet<GraphCid> {
        if self.inner.len() <= max {
            self.inner.clone()
        } else {
            self.inner.iter().take(max).cloned().collect()
        }
    }
}

/// A scored discovery in the local pool.
struct LocalDiscovery {
    graph: AdjacencyMatrix,
    score: GraphScore,
    cid: GraphCid,
}

/// Observer that forwards progress to the viz bridge and handles cancellation.
struct EngineObserver {
    cancelled: Arc<AtomicBool>,
    viz: Option<Arc<dyn VizBridge>>,
}

impl SearchObserver for EngineObserver {
    fn on_progress(&self, info: &ProgressInfo) {
        if let Some(ref v) = self.viz {
            v.on_progress(&info.graph, info);
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// The worker engine. Owns the full lifecycle of search, scoring, and submission.
pub struct WorkerEngine;

impl WorkerEngine {
    /// Run the engine loop. This is the main entry point.
    pub async fn run(
        config: EngineConfig,
        strategies: Vec<Box<dyn SearchStrategy>>,
        client: Option<ServerClient>,
        viz: Option<Arc<dyn VizBridge>>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), WorkerError> {
        let strategies: Vec<Arc<dyn SearchStrategy>> =
            strategies.into_iter().map(Arc::from).collect();

        let mut rng = SmallRng::from_entropy();
        let mut pool_rng = SmallRng::from_entropy();
        let mut consecutive_failures = 0u32;
        let k = config.k;
        let ell = config.ell;
        let target_n = config.n;

        let mut known = KnownCids::new();
        let mut threshold = AdmissionThreshold::open();
        let mut cid_sync_cursor: Option<String> = None;
        let mut leaderboard_total: u32 = 0;
        let mut server_pool: Vec<AdjacencyMatrix> = Vec::new();

        // Local discovery pool: accumulates best discoveries across rounds.
        // Used as seed graph source for non-Leaderboard init modes.
        let mut local_pool: Vec<LocalDiscovery> = Vec::new();
        let local_pool_capacity = config.collector_capacity.max(100);

        let is_online = !config.offline && client.is_some();
        let use_server_pool = matches!(config.init_mode, InitMode::Leaderboard);

        if !is_online {
            info!(k, ell, target_n, "starting offline search (no server)");
        }

        loop {
            if *shutdown.borrow() {
                info!("shutdown signal received, exiting");
                return Ok(());
            }

            // ── Sync with server (online only) ──────────────────────
            if is_online {
                let client = client.as_ref().unwrap();

                // Fetch threshold
                match client.get_threshold(k, ell, target_n).await {
                    Ok(resp) => {
                        info!(
                            k, ell, target_n,
                            entries = resp.entry_count,
                            capacity = resp.capacity,
                            worst_t1 = ?resp.worst_tier1_max,
                            "fetched leaderboard threshold"
                        );
                        leaderboard_total = resp.entry_count;
                        threshold = AdmissionThreshold::from_response(&resp);
                    }
                    Err(e) => warn!("failed to fetch threshold: {e}"),
                }

                // Incremental CID sync
                match client
                    .get_leaderboard_cids_since(k, ell, target_n, cid_sync_cursor.as_deref())
                    .await
                {
                    Ok(resp) => {
                        if !resp.cids.is_empty() {
                            known.add_from_hex(&resp.cids);
                        }
                        if let Some(ref ts) = resp.last_updated {
                            cid_sync_cursor = Some(ts.clone());
                        }
                        info!(
                            known = known.len(),
                            new_cids = resp.cids.len(),
                            total = resp.total,
                            "synced leaderboard CIDs"
                        );
                    }
                    Err(e) => warn!("failed to sync leaderboard CIDs: {e}"),
                }

                // Refresh server pool (only in Leaderboard init mode)
                if use_server_pool {
                    let max_offset =
                        leaderboard_total.saturating_sub(config.leaderboard_sample_size);
                    let offset = if max_offset == 0 || config.sample_bias >= 1.0 {
                        0
                    } else {
                        let u: f64 = pool_rng.gen();
                        let biased = u.powf(1.0 / (1.0 - config.sample_bias * 0.95));
                        (biased * max_offset as f64) as u32
                    };

                    match client
                        .get_leaderboard_graphs(
                            k,
                            ell,
                            target_n,
                            config.leaderboard_sample_size,
                            offset,
                        )
                        .await
                    {
                        Ok(rgxfs) => {
                            server_pool = rgxfs
                                .iter()
                                .filter_map(|r| rgxf::from_json(r).ok())
                                .collect();
                            info!(count = server_pool.len(), offset, "refreshed server seed pool");
                        }
                        Err(e) => warn!("failed to fetch leaderboard graphs: {e}"),
                    }
                }
            }

            info!(k, ell, target_n, "starting search round");

            let mut found = false;

            for strategy in &strategies {
                if *shutdown.borrow() {
                    info!("shutdown signal received, exiting");
                    return Ok(());
                }

                let start = Instant::now();
                let strategy_id = strategy.id().to_string();
                let max_iters = config.max_iters;

                info!(strategy = %strategy_id, target_n, max_iters, "running search");

                // ── Determine seed graph ────────────────────────────
                let seed_graph = if use_server_pool {
                    // Leaderboard mode: sample from server pool
                    init::sample_init_graph(
                        &server_pool,
                        config.sample_bias,
                        target_n,
                        config.noise_flips,
                        &mut rng,
                    )
                } else if !local_pool.is_empty() {
                    // Self-learning mode: sample from local pool
                    let local_graphs: Vec<AdjacencyMatrix> =
                        local_pool.iter().map(|d| d.graph.clone()).collect();
                    init::sample_init_graph(
                        &local_graphs,
                        config.sample_bias,
                        target_n,
                        config.noise_flips,
                        &mut rng,
                    )
                } else {
                    // First round or no discoveries yet: use init mode
                    init::make_init_graph(&config.init_mode, target_n, &mut rng)
                };

                let job = SearchJob {
                    k,
                    ell,
                    n: target_n,
                    max_iters,
                    seed: rng.gen(),
                    init_graph: Some(seed_graph),
                    config: config.strategy_config.clone(),
                    known_cids: known.snapshot_trimmed(config.max_known_cids),
                    max_known_cids: config.max_known_cids,
                };

                let cancel_flag = Arc::new(AtomicBool::new(false));
                let cancel_for_search = cancel_flag.clone();
                let strategy_clone = Arc::clone(strategy);
                let viz_for_observer = viz.clone();

                let mut search_handle = tokio::task::spawn_blocking(move || {
                    let observer = EngineObserver {
                        cancelled: cancel_for_search,
                        viz: viz_for_observer,
                    };
                    strategy_clone.search(&job, &observer)
                });

                let submit_interval = Duration::from_secs(30);
                let mut last_submit = Instant::now();
                let mut shutting_down = false;

                let result = loop {
                    tokio::select! {
                        result = &mut search_handle => {
                            break result.unwrap();
                        }
                        _ = shutdown.changed(), if !shutting_down => {
                            info!("shutdown signal received, cancelling search...");
                            cancel_flag.store(true, Ordering::Relaxed);
                            shutting_down = true;
                        }
                        _ = tokio::time::sleep(submit_interval.saturating_sub(last_submit.elapsed())) => {
                            if *shutdown.borrow() && !shutting_down {
                                info!("shutdown signal received, cancelling search...");
                                cancel_flag.store(true, Ordering::Relaxed);
                                shutting_down = true;
                            }
                            last_submit = Instant::now();
                        }
                    }
                };

                let elapsed = start.elapsed();

                if shutting_down {
                    info!(
                        strategy = %strategy_id,
                        iterations = result.iterations_used,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "search interrupted by shutdown"
                    );
                }

                // ── Score discoveries (platform's job) ──────────────
                // Skip expensive scoring if shutting down — just exit fast
                let mut scored: Vec<LocalDiscovery> = Vec::new();
                if shutting_down {
                    info!("shutdown complete — skipping post-search scoring");
                    return Ok(());
                }
                for raw in &result.discoveries {
                    let sr = compute_score_canonical(&raw.graph);
                    let canonical_cid = compute_cid(&sr.canonical_graph);

                    if known.contains(&canonical_cid) {
                        continue;
                    }
                    known.insert(canonical_cid.clone());

                    // Forward to viz
                    if let Some(ref v) = viz {
                        v.on_discovery(
                            &sr.canonical_graph,
                            target_n,
                            &strategy_id,
                            raw.iteration,
                            sr.score.clone(),
                        );
                    }

                    scored.push(LocalDiscovery {
                        graph: sr.canonical_graph,
                        score: sr.score,
                        cid: canonical_cid,
                    });
                }

                // Also score the final best graph if valid
                if result.valid {
                    if let Some(ref best) = result.best_graph {
                        let sr = compute_score_canonical(best);
                        let canonical_cid = compute_cid(&sr.canonical_graph);
                        if !known.contains(&canonical_cid) {
                            known.insert(canonical_cid.clone());
                            if let Some(ref v) = viz {
                                v.on_discovery(
                                    &sr.canonical_graph,
                                    target_n,
                                    &strategy_id,
                                    result.iterations_used,
                                    sr.score.clone(),
                                );
                            }
                            scored.push(LocalDiscovery {
                                graph: sr.canonical_graph,
                                score: sr.score,
                                cid: canonical_cid,
                            });
                        }
                    }
                }

                scored.sort_by(|a, b| a.score.cmp(&b.score));
                scored.truncate(config.collector_capacity);

                // ── Feed local pool (self-learning) ─────────────────
                if !use_server_pool {
                    for discovery in &scored {
                        // Check if this graph is better than the worst in the local pool
                        let dominated = local_pool.len() >= local_pool_capacity
                            && local_pool
                                .last()
                                .map(|worst| discovery.score >= worst.score)
                                .unwrap_or(false);

                        if dominated {
                            continue;
                        }

                        // Check CID uniqueness in local pool
                        if local_pool.iter().any(|d| d.cid == discovery.cid) {
                            continue;
                        }

                        // Insert sorted by score
                        let pos = local_pool
                            .binary_search_by(|d| d.score.cmp(&discovery.score))
                            .unwrap_or_else(|p| p);

                        local_pool.insert(
                            pos,
                            LocalDiscovery {
                                graph: discovery.graph.clone(),
                                score: discovery.score.clone(),
                                cid: discovery.cid.clone(),
                            },
                        );

                        if local_pool.len() > local_pool_capacity {
                            local_pool.pop();
                        }
                    }

                    if !local_pool.is_empty() {
                        info!(
                            local_pool_size = local_pool.len(),
                            best_score = ?local_pool.first().map(|d| format!(
                                "t1=({},{}), gap={}", d.score.c_omega.max(d.score.c_alpha),
                                d.score.c_omega.min(d.score.c_alpha), d.score.goodman_gap
                            )),
                            "local pool updated"
                        );
                    }
                }

                if !scored.is_empty() {
                    info!(
                        strategy = %strategy_id,
                        target_n,
                        iterations = result.iterations_used,
                        elapsed_ms = elapsed.as_millis() as u64,
                        discoveries = scored.len(),
                        "search completed with discoveries"
                    );
                } else if !shutting_down {
                    if result.valid {
                        info!(
                            strategy = %strategy_id,
                            target_n,
                            iterations = result.iterations_used,
                            elapsed_ms = elapsed.as_millis() as u64,
                            "found valid graph (all duplicates)"
                        );
                    } else {
                        warn!(
                            strategy = %strategy_id,
                            target_n,
                            iterations = result.iterations_used,
                            elapsed_ms = elapsed.as_millis() as u64,
                            "no valid graph found"
                        );
                    }
                }

                // ── Submit to server (online only) ──────────────────
                if is_online && !scored.is_empty() {
                    let client = client.as_ref().unwrap();
                    let mut submitted = 0usize;
                    let mut admitted = 0usize;
                    let mut skipped = 0usize;

                    for discovery in &scored {
                        if !threshold.would_admit(&discovery.score) {
                            debug!(
                                graph_cid = %discovery.cid.to_hex(),
                                "skipping — below threshold"
                            );
                            skipped += 1;
                            continue;
                        }

                        let rgxf_json = rgxf::to_json(&discovery.graph);
                        match client.submit(k, ell, target_n, rgxf_json).await {
                            Ok(resp) => {
                                let was_admitted = resp.admitted.unwrap_or(false);
                                info!(
                                    graph_cid = %resp.graph_cid,
                                    verdict = %resp.verdict,
                                    admitted = was_admitted,
                                    rank = ?resp.rank,
                                    "submitted graph"
                                );
                                known.insert_hex(&resp.graph_cid);
                                submitted += 1;
                                if was_admitted {
                                    admitted += 1;
                                    info!(
                                        "admitted to leaderboard! rank={}",
                                        resp.rank.unwrap_or(0)
                                    );
                                }
                            }
                            Err(e) => {
                                error!(
                                    graph_cid = %discovery.cid.to_hex(),
                                    "submit failed: {e}"
                                );
                            }
                        }
                    }

                    if submitted > 0 || skipped > 0 {
                        info!(submitted, admitted, skipped, "submission batch");
                    }
                    if submitted > 0 {
                        found = true;
                        consecutive_failures = 0;
                    }
                } else if !scored.is_empty() {
                    found = true;
                    info!(
                        discoveries = scored.len(),
                        "discoveries found (offline, not submitted)"
                    );
                }

            }

            if !found {
                consecutive_failures += 1;
                if config.no_backoff {
                    warn!(
                        consecutive_failures, target_n,
                        "all strategies failed, retrying immediately"
                    );
                } else {
                    let backoff_secs = (2u64.pow(consecutive_failures.min(5))).min(60);
                    warn!(
                        consecutive_failures, backoff_secs, target_n,
                        "all strategies failed, backing off"
                    );
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        _ = shutdown.changed() => {
                            info!("shutdown signal received during backoff");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}
