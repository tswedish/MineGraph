use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use tokio::sync::watch;
use tracing::info;

use ramseynet_graph::AdjacencyMatrix;
use ramseynet_strategies::default_strategies;
use ramseynet_verifier::scoring::GraphScore;
use ramseynet_worker::viz::{SearchSnapshot, VizHandle};
use ramseynet_worker_api::ProgressInfo;
use ramseynet_worker_core::client::ServerClient;
use ramseynet_worker_core::engine::EngineConfig;
use ramseynet_worker_core::{InitMode, VizBridge, WorkerEngine};

/// VizBridge implementation that forwards engine events to the VizHandle.
struct VizBridgeImpl {
    handle: Arc<VizHandle>,
    /// Throttle: last time we sent a snapshot (only send at ~20fps).
    last_snapshot: Mutex<Instant>,
    /// EMA state for throughput: (last_iteration, last_instant, smoothed).
    ema: Mutex<(u64, Instant, f64)>,
}

impl VizBridgeImpl {
    fn new(handle: Arc<VizHandle>) -> Self {
        let now = Instant::now();
        Self {
            handle,
            last_snapshot: Mutex::new(now),
            ema: Mutex::new((0, now, 0.0)),
        }
    }
}

/// EMA smoothing factor.
const EMA_ALPHA: f64 = 0.3;

impl VizBridge for VizBridgeImpl {
    fn on_progress(&self, graph: &AdjacencyMatrix, info: &ProgressInfo) {
        // Throttle to ~20fps
        let now = Instant::now();
        {
            let mut last = self.last_snapshot.lock().unwrap();
            if now.duration_since(*last).as_millis() < 50 {
                return;
            }
            *last = now;
        }

        let elapsed_ms = self.handle.elapsed_ms();

        // EMA throughput
        let throughput = {
            let mut ema = self.ema.lock().unwrap();
            let dt = now.duration_since(ema.1).as_secs_f64();
            let d_iters = info.iteration.saturating_sub(ema.0);
            let instant_rate = if dt > 0.0 {
                d_iters as f64 / dt
            } else {
                ema.2
            };
            let smoothed = if info.iteration < ema.0 || ema.2 == 0.0 {
                instant_rate
            } else {
                EMA_ALPHA * instant_rate + (1.0 - EMA_ALPHA) * ema.2
            };
            *ema = (info.iteration, now, smoothed);
            smoothed
        };

        let snapshot = SearchSnapshot {
            graph: ramseynet_graph::rgxf::to_json(graph),
            n: info.n,
            k: info.k,
            ell: info.ell,
            strategy: info.strategy.clone(),
            iteration: info.iteration,
            max_iters: info.max_iters,
            valid: info.valid,
            edges: graph.num_edges() as u32,
            violation_score: info.violation_score,
            k_cliques: info.k_cliques,
            ell_indsets: info.ell_indsets,
            elapsed_ms,
            throughput,
        };
        self.handle.update_snapshot(snapshot);
    }

    fn on_discovery(
        &self,
        graph: &AdjacencyMatrix,
        n: u32,
        strategy: &str,
        iteration: u64,
        score: GraphScore,
    ) {
        self.handle
            .submit_discovery(graph, n, strategy, iteration, false, score);
    }
}

#[derive(Parser)]
#[command(name = "ramseynet-worker", about = "RamseyNet search worker")]
struct Cli {
    /// Server URL (e.g. http://localhost:3001)
    #[arg(long, default_value = "http://localhost:3001")]
    server: String,

    /// Ramsey parameter k (clique size)
    #[arg(long)]
    k: u32,

    /// Ramsey parameter ell (independent set size)
    #[arg(long)]
    ell: u32,

    /// Target vertex count n
    #[arg(long)]
    n: u32,

    /// Search strategy (currently: tree)
    #[arg(long, default_value = "tree")]
    strategy: String,

    /// Maximum iterations per search attempt
    #[arg(long, default_value = "100000")]
    max_iters: u64,

    /// Port for live visualization web server
    #[arg(long)]
    viz_port: Option<u16>,

    /// Disable backoff delay between failed search rounds
    #[arg(long)]
    no_backoff: bool,

    /// Offline mode: search continuously without a server
    #[arg(long)]
    offline: bool,

    /// Graph initialization: paley, perturbed-paley, random, leaderboard
    #[arg(long, default_value = "perturbed-paley")]
    init: String,

    /// Number of random edge flips for seed graph noise (default: auto)
    #[arg(long)]
    noise_flips: Option<u32>,

    /// Sampling bias for leaderboard/pool init: 0.0 = uniform, 1.0 = top-heavy
    #[arg(long, default_value = "0.5")]
    sample_bias: f64,

    /// How many graphs to fetch from the server for leaderboard seeding
    #[arg(long, default_value = "100")]
    leaderboard_sample_size: u32,

    /// Per-strategy discovery buffer capacity
    #[arg(long, default_value = "1000")]
    collector_capacity: usize,

    /// Maximum known CIDs to pass to strategies for deduplication
    #[arg(long, default_value = "10000")]
    max_known_cids: usize,

    /// Beam width for tree search
    #[arg(long, default_value = "100")]
    beam_width: usize,

    /// Maximum search depth for tree search
    #[arg(long, default_value = "10")]
    max_depth: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Parse init mode
    let init_mode = match cli.init.as_str() {
        "paley" => InitMode::Paley,
        "perturbed-paley" => InitMode::PerturbedPaley,
        "random" => InitMode::Random,
        "leaderboard" => InitMode::Leaderboard,
        other => anyhow::bail!(
            "unknown init mode: {other} (use paley, perturbed-paley, random, leaderboard)"
        ),
    };

    info!(
        server = %cli.server,
        k = cli.k,
        ell = cli.ell,
        n = cli.n,
        strategy = %cli.strategy,
        init = %cli.init,
        "starting ramseynet worker"
    );

    // Validate strategy
    let strategies = default_strategies();
    let available: Vec<&str> = strategies.iter().map(|s| s.id()).collect();
    if cli.strategy != "all" && !available.contains(&cli.strategy.as_str()) {
        anyhow::bail!(
            "unknown strategy: {} (available: {})",
            cli.strategy,
            available.join(", ")
        );
    }

    let strategies: Vec<_> = if cli.strategy == "all" {
        strategies
    } else {
        strategies
            .into_iter()
            .filter(|s| s.id() == cli.strategy)
            .collect()
    };

    // Auto-compute noise flips
    let n = cli.n;
    let num_edges = n * (n - 1) / 2;
    let noise_flips = cli
        .noise_flips
        .unwrap_or(((num_edges as f64).sqrt() / 2.0).ceil() as u32);

    let strategy_config = serde_json::json!({
        "beam_width": cli.beam_width,
        "max_depth": cli.max_depth,
    });

    let config = EngineConfig {
        k: cli.k,
        ell: cli.ell,
        n: cli.n,
        max_iters: cli.max_iters,
        no_backoff: cli.no_backoff,
        offline: cli.offline,
        sample_bias: cli.sample_bias,
        leaderboard_sample_size: cli.leaderboard_sample_size,
        collector_capacity: cli.collector_capacity,
        max_known_cids: cli.max_known_cids,
        noise_flips,
        init_mode,
        strategy_config,
    };

    let client = if cli.offline {
        None
    } else {
        Some(ServerClient::new(&cli.server))
    };

    // Graceful shutdown on Ctrl+C
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Ctrl+C received, shutting down...");
        let _ = shutdown_tx.send(true);
    });

    // Start viz server if requested
    let viz: Option<Arc<dyn VizBridge>> = if let Some(port) = cli.viz_port {
        let handle = Arc::new(VizHandle::new());
        let viz_shutdown = shutdown_rx.clone();
        let viz_for_server = Arc::clone(&handle);
        tokio::spawn(async move {
            ramseynet_worker::viz::server::start_viz_server(port, viz_for_server, viz_shutdown)
                .await;
        });
        info!("viz server at http://localhost:{port}");
        Some(Arc::new(VizBridgeImpl::new(handle)))
    } else {
        None
    };

    WorkerEngine::run(config, strategies, client, viz, shutdown_rx).await?;

    Ok(())
}
