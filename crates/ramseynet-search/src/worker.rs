use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::rngs::SmallRng;
use rand::SeedableRng;
use ramseynet_graph::{compute_cid, rgxf, AdjacencyMatrix};
use ramseynet_types::GraphCid;
use ramseynet_verifier::scoring::{compute_score_canonical, GraphScore};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::client::{ServerClient, ThresholdResponse};
use crate::error::SearchError;
use crate::search::Searcher;
use crate::viz::{
    CollectorObserver, Discovery, DiscoveryCollector, KnownCids, VizHandle, VizObserver,
};

// ── Admission threshold ─────────────────────────────────────────────

/// Cached admission threshold from the server. Used to pre-filter
/// candidates before submitting — if a graph's score wouldn't beat
/// the worst entry on a full leaderboard, don't waste an HTTP round-trip.
struct AdmissionThreshold {
    /// Reconstructed worst score for comparison. None if board isn't full
    /// (meaning any valid graph will be admitted).
    worst_score: Option<GraphScore>,
}

impl AdmissionThreshold {
    /// No threshold — accept everything (board empty or unknown).
    fn open() -> Self {
        Self {
            worst_score: None,
        }
    }

    /// Build from server threshold response.
    fn from_response(resp: &ThresholdResponse) -> Self {
        let worst_score = if resp.entry_count >= resp.capacity {
            // Board is full — need to beat the worst entry
            match (
                resp.worst_tier1_max,
                resp.worst_tier1_min,
                resp.worst_goodman_gap,
                resp.worst_tier2_aut,
                resp.worst_tier3_cid.as_ref(),
            ) {
                (Some(t1_max), Some(t1_min), Some(goodman_gap), Some(t2_aut), Some(t3_cid)) => {
                    // Reconstruct a GraphScore for comparison.
                    // We use n=0 and set triangles to produce the correct goodman_gap.
                    // Since goodman_minimum(0) = 0, goodman_gap = triangles + 0 - 0.
                    match GraphCid::from_hex(t3_cid) {
                        Ok(cid) => Some(GraphScore::new(
                            0,
                            0,
                            0,
                            t1_max,
                            t1_min,
                            goodman_gap,
                            0,
                            t2_aut,
                            cid,
                        )),
                        Err(_) => None, // Can't parse CID — skip threshold filtering
                    }
                }
                _ => None,
            }
        } else {
            None // Board not full — any valid graph is admitted
        };

        Self { worst_score }
    }

    /// Would a graph with this score be admitted?
    fn would_admit(&self, score: &GraphScore) -> bool {
        match &self.worst_score {
            None => true, // Board not full — everything is admitted
            Some(worst) => {
                // Score ordering: lower is better. A candidate is admitted
                // if it's strictly better than (less than) the worst entry.
                score < worst
            }
        }
    }
}

/// Configuration for the worker loop.
pub struct WorkerConfig {
    pub k: u32,
    pub ell: u32,
    pub n: u32,
    pub max_iters: u64,
    pub no_backoff: bool,
    pub offline: bool,
    /// Shared pool for leaderboard-seeded init strategy. When `Some`, the
    /// worker refreshes this pool from the server each round.
    pub leaderboard_pool: Option<Arc<Mutex<Vec<AdjacencyMatrix>>>>,
    /// How many graphs to fetch from the server for leaderboard seed pool.
    pub leaderboard_sample_size: u32,
    /// Per-strategy discovery buffer capacity.
    pub collector_capacity: usize,
}

// ── Periodic submission ─────────────────────────────────────────────

/// Drain the collector and submit discoveries to the server, skipping
/// any whose canonical CID is already known (from prior rounds, server
/// leaderboard, or earlier submissions in this batch).
async fn submit_discoveries(
    client: &ServerClient,
    collector: &DiscoveryCollector,
    known: &KnownCids,
    threshold: &AdmissionThreshold,
    k: u32,
    ell: u32,
    n: u32,
) -> (usize, usize, usize) {
    let discoveries = collector.drain();
    if discoveries.is_empty() {
        return (0, 0, 0);
    }

    // Immediately mark all drained CIDs as known, closing the race window
    // where the search thread (still running in spawn_blocking) could
    // re-discover and re-push the same canonical CID into the now-empty
    // collector before the server responds.
    for d in &discoveries {
        known.insert(d.cid.clone());
    }

    let mut submitted = 0usize;
    let mut admitted = 0usize;
    let mut skipped = 0usize;

    for discovery in &discoveries {
        let cid_hex = discovery.cid.to_hex();

        // Pre-check: would this score beat the server's leaderboard threshold?
        // If not, skip the HTTP round-trip entirely.
        if !threshold.would_admit(&discovery.score) {
            debug!(
                graph_cid = %cid_hex,
                "skipping submission — score below leaderboard threshold"
            );
            skipped += 1;
            continue;
        }

        let rgxf_json = rgxf::to_json(&discovery.graph);
        match client.submit(k, ell, n, rgxf_json).await {
            Ok(resp) => {
                let was_admitted = resp.admitted.unwrap_or(false);
                info!(
                    graph_cid = %resp.graph_cid,
                    verdict = %resp.verdict,
                    admitted = was_admitted,
                    rank = ?resp.rank,
                    "submitted graph"
                );
                // Also mark the server's returned CID (should match, but belt-and-suspenders)
                known.insert_hex(&resp.graph_cid);
                submitted += 1;

                if was_admitted {
                    admitted += 1;
                    info!("admitted to leaderboard! rank={}", resp.rank.unwrap_or(0));
                }
            }
            Err(e) => {
                error!(graph_cid = %cid_hex, "submit failed: {e}");
            }
        }
    }

    (submitted, admitted, skipped)
}

// ── Main worker loop ────────────────────────────────────────────────

/// Run the search worker loop.
pub async fn run_worker(
    client: ServerClient,
    searchers: Vec<Box<dyn Searcher>>,
    config: WorkerConfig,
    mut shutdown: watch::Receiver<bool>,
    viz_handle: Option<Arc<VizHandle>>,
) -> Result<(), SearchError> {
    if config.offline {
        return run_worker_offline(searchers, config, shutdown, viz_handle).await;
    }

    let searchers: Vec<Arc<dyn Searcher>> = searchers.into_iter().map(Arc::from).collect();
    let mut rng = SmallRng::from_entropy();
    let mut consecutive_failures = 0u32;
    let k = config.k;
    let ell = config.ell;
    let target_n = config.n;

    // Persistent known-CID set: survives across rounds, strategies, and
    // periodic submission batches. Prevents re-submitting any graph that
    // has been seen before (on the leaderboard, submitted, or discovered).
    let known = KnownCids::new();

    // Admission threshold: updated each round from the server. Used to
    // pre-filter candidates so we only submit graphs that would actually
    // beat the leaderboard.
    let mut threshold = AdmissionThreshold::open();

    // Tracks the last_updated timestamp for incremental CID sync.
    // None on first round → full sync. Subsequent rounds use the
    // timestamp to fetch only newly admitted CIDs.
    let mut cid_sync_cursor: Option<String> = None;

    loop {
        // Check shutdown
        if *shutdown.borrow() {
            info!("shutdown signal received, exiting");
            return Ok(());
        }

        // Fetch threshold and update admission filter
        match client.get_threshold(k, ell, target_n).await {
            Ok(resp) => {
                info!(
                    k, ell, target_n,
                    entries = resp.entry_count,
                    capacity = resp.capacity,
                    worst_t1 = ?resp.worst_tier1_max,
                    "fetched leaderboard threshold"
                );
                threshold = AdmissionThreshold::from_response(&resp);
            }
            Err(e) => {
                warn!("failed to fetch threshold: {e}");
            }
        }

        // Incremental CID sync from server leaderboard
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
            Err(e) => {
                warn!("failed to sync leaderboard CIDs: {e}");
            }
        }

        // Refresh leaderboard pool if using leaderboard init
        if let Some(ref pool) = config.leaderboard_pool {
            match client
                .get_leaderboard_graphs(k, ell, target_n, config.leaderboard_sample_size)
                .await
            {
                Ok(rgxfs) => {
                    let graphs: Vec<AdjacencyMatrix> = rgxfs
                        .iter()
                        .filter_map(|r| rgxf::from_json(r).ok())
                        .collect();
                    let count = graphs.len();
                    *pool.lock().unwrap() = graphs;
                    info!(count, "refreshed leaderboard seed pool");
                }
                Err(e) => {
                    warn!("failed to fetch leaderboard graphs: {e}");
                }
            }
        }

        info!(k, ell, target_n, "starting search round");

        let mut found = false;

        for searcher in &searchers {
            if *shutdown.borrow() {
                info!("shutdown signal received, exiting");
                return Ok(());
            }

            let start = Instant::now();
            let strategy = searcher.name();
            let max_iters = config.max_iters;

            info!(strategy, target_n, max_iters, "running search");

            // Collector with cross-round dedup via known CIDs
            let collector = DiscoveryCollector::with_known(config.collector_capacity, known.clone());
            let collector_for_search = collector.clone();
            let collector_for_submit = collector.clone();
            let n = target_n;
            let searcher = Arc::clone(searcher);
            let mut search_rng = SmallRng::from_rng(&mut rng).unwrap();
            let viz = viz_handle.clone();

            // Shared cancellation flag — set on Ctrl+C so the search
            // bails out within ~100 iterations instead of running to max_iters.
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let cancel_for_search = cancel_flag.clone();

            // Spawn the search in a blocking thread
            let mut search_handle = tokio::task::spawn_blocking(move || {
                let viz_obs = viz.map(VizObserver::new);
                let obs = CollectorObserver::with_cancel(
                    collector_for_search, viz_obs, cancel_for_search,
                );
                searcher.search(n, k, ell, max_iters, &mut search_rng, &obs)
            });

            // While search is running, periodically drain and submit discoveries
            let submit_interval = Duration::from_secs(30);
            let mut last_submit = Instant::now();
            let mut shutting_down = false;

            loop {
                tokio::select! {
                    result = &mut search_handle => {
                        // Search completed (normally or via cancellation)
                        let result = result.unwrap();
                        let elapsed = start.elapsed();

                        if shutting_down {
                            info!(
                                strategy,
                                iterations = result.iterations,
                                elapsed_ms = elapsed.as_millis() as u64,
                                "search interrupted by shutdown"
                            );
                        }

                        // If final result is valid, add it to the collector
                        if result.valid {
                            let score_result = compute_score_canonical(&result.graph);
                            let canonical_cid = compute_cid(&score_result.canonical_graph);
                            collector_for_submit.push(Discovery {
                                graph: score_result.canonical_graph.clone(),
                                score: score_result.score.clone(),
                                cid: canonical_cid,
                            });

                            if let Some(ref vh) = viz_handle {
                                if let Some(entry) = vh.submit_discovery(
                                    &score_result.canonical_graph, target_n, strategy, result.iterations,
                                    false, score_result.score,
                                ) {
                                    info!(
                                        strategy,
                                        target_n,
                                        iterations = result.iterations,
                                        edges = result.graph.num_edges(),
                                        elapsed_ms = elapsed.as_millis() as u64,
                                        omega = entry.score.omega,
                                        alpha = entry.score.alpha,
                                        c_omega = entry.score.c_omega,
                                        c_alpha = entry.score.c_alpha,
                                        aut_order = entry.score.aut_order,
                                        rank = entry.rank,
                                        "found valid graph!"
                                    );
                                }
                            } else if !shutting_down {
                                info!(
                                    strategy,
                                    target_n,
                                    iterations = result.iterations,
                                    edges = result.graph.num_edges(),
                                    elapsed_ms = elapsed.as_millis() as u64,
                                    "found valid graph!"
                                );
                            }
                        } else if !shutting_down {
                            warn!(
                                strategy,
                                target_n,
                                iterations = result.iterations,
                                elapsed_ms = elapsed.as_millis() as u64,
                                "no valid graph found"
                            );
                        }

                        // Final drain + submit any remaining discoveries
                        let (sub, adm, skip) = submit_discoveries(
                            &client, &collector_for_submit, &known, &threshold,
                            k, ell, target_n,
                        ).await;
                        if sub > 0 || skip > 0 {
                            info!(submitted = sub, admitted = adm, skipped = skip, "final submission batch");
                        }
                        if sub > 0 {
                            found = true;
                        }

                        if shutting_down {
                            info!("shutdown complete — submitted remaining discoveries");
                            return Ok(());
                        }

                        break; // Move to next strategy
                    }
                    _ = shutdown.changed(), if !shutting_down => {
                        // Ctrl+C received — signal the search to stop ASAP
                        info!("shutdown signal received, cancelling search...");
                        cancel_flag.store(true, Ordering::Relaxed);
                        shutting_down = true;
                        // Don't break — wait for the search to finish (should be fast)
                        // The next iteration will pick up the search_handle completing.
                    }
                    _ = tokio::time::sleep(submit_interval.saturating_sub(last_submit.elapsed())) => {
                        // Check shutdown between periodic submissions
                        if *shutdown.borrow() && !shutting_down {
                            info!("shutdown signal received, cancelling search...");
                            cancel_flag.store(true, Ordering::Relaxed);
                            shutting_down = true;
                        }

                        // Periodic drain + submit while search is still running
                        let (sub, adm, skip) = submit_discoveries(
                            &client, &collector_for_submit, &known, &threshold,
                            k, ell, target_n,
                        ).await;
                        if sub > 0 || skip > 0 {
                            info!(submitted = sub, admitted = adm, skipped = skip, "periodic submission batch");
                        }
                        if sub > 0 {
                            found = true;
                            consecutive_failures = 0;
                        }
                        last_submit = Instant::now();
                    }
                }
            }
        }

        if !found {
            consecutive_failures += 1;

            if config.no_backoff {
                warn!(consecutive_failures, target_n, "all strategies failed, retrying immediately");
            } else {
                let backoff_secs = (2u64.pow(consecutive_failures.min(5))).min(60);
                warn!(
                    consecutive_failures,
                    backoff_secs,
                    target_n,
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

/// Offline worker loop — no server, searches continuously and pins to viz.
async fn run_worker_offline(
    searchers: Vec<Box<dyn Searcher>>,
    config: WorkerConfig,
    shutdown: watch::Receiver<bool>,
    viz_handle: Option<Arc<VizHandle>>,
) -> Result<(), SearchError> {
    let k = config.k;
    let ell = config.ell;
    let target_n = config.n;

    info!(
        k, ell, target_n,
        "starting offline search (no server)"
    );

    let searchers: Vec<Arc<dyn Searcher>> = searchers.into_iter().map(Arc::from).collect();
    let mut rng = SmallRng::from_entropy();
    let mut round = 0u64;

    // Even in offline mode, keep a known-CID set so we don't re-discover
    // the same isomorphism classes across rounds.
    let known = KnownCids::new();

    loop {
        if *shutdown.borrow() {
            info!("shutdown signal received, exiting");
            return Ok(());
        }

        round += 1;

        for searcher in &searchers {
            if *shutdown.borrow() {
                info!("shutdown signal received, exiting");
                return Ok(());
            }

            let start = Instant::now();
            let strategy = searcher.name();
            let max_iters = config.max_iters;

            let n = target_n;
            let searcher = Arc::clone(searcher);
            let mut search_rng = SmallRng::from_rng(&mut rng).unwrap();
            let viz = viz_handle.clone();
            let collector = DiscoveryCollector::with_known(config.collector_capacity, known.clone());
            let collector_for_search = collector.clone();
            let (result, score) = tokio::task::spawn_blocking(move || {
                let viz_obs = viz.map(VizObserver::new);
                let obs = CollectorObserver::new(collector_for_search, viz_obs);
                let result = searcher.search(n, k, ell, max_iters, &mut search_rng, &obs);
                let score = if result.valid {
                    let sr = compute_score_canonical(&result.graph);
                    Some(sr.score)
                } else {
                    None
                };
                (result, score)
            })
            .await
            .unwrap();

            let elapsed = start.elapsed();
            // Discoveries already forwarded to viz via CollectorObserver; just drain to drop
            let _discoveries = collector.drain();

            if let Some(score) = score {
                // Compute canonical form for consistent viz display
                let (canonical_graph, _) = ramseynet_verifier::canonical_form(&result.graph);
                if let Some(ref vh) = viz_handle {
                    if let Some(entry) = vh.submit_discovery(
                        &canonical_graph, target_n, strategy, result.iterations,
                        false, score,
                    ) {
                        info!(
                            strategy,
                            target_n,
                            round,
                            iterations = result.iterations,
                            edges = result.graph.num_edges(),
                            elapsed_ms = elapsed.as_millis() as u64,
                            omega = entry.score.omega,
                            alpha = entry.score.alpha,
                            c_omega = entry.score.c_omega,
                            c_alpha = entry.score.c_alpha,
                            aut_order = entry.score.aut_order,
                            rank = entry.rank,
                            "found valid graph (offline)"
                        );
                    }
                } else {
                    info!(
                        strategy,
                        target_n,
                        round,
                        iterations = result.iterations,
                        edges = result.graph.num_edges(),
                        elapsed_ms = elapsed.as_millis() as u64,
                        "found valid graph (offline)"
                    );
                }
            } else {
                warn!(
                    strategy,
                    target_n,
                    round,
                    iterations = result.iterations,
                    elapsed_ms = elapsed.as_millis() as u64,
                    "no valid graph found"
                );
            }
        }
    }
}
