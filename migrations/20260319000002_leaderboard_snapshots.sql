-- Periodic leaderboard score snapshots for history tracking.
-- Captured by a server background task every ~10 minutes.

CREATE TABLE IF NOT EXISTS leaderboard_snapshots (
    id          BIGSERIAL PRIMARY KEY,
    n           INTEGER NOT NULL,
    entry_count INTEGER NOT NULL,
    -- Cumulative "total score" = sum(goodman_gap) + sum(all clique counts)
    -- Same metric as the UI's "Score" badge. Lower = better leaderboard.
    total_score BIGINT NOT NULL DEFAULT 0,
    -- Score statistics
    best_gap    DOUBLE PRECISION,   -- min goodman_gap (#1 entry)
    worst_gap   DOUBLE PRECISION,   -- max goodman_gap (worst entry)
    median_gap  DOUBLE PRECISION,
    avg_gap     DOUBLE PRECISION,
    best_aut    DOUBLE PRECISION,   -- max aut_order (best symmetry)
    avg_aut     DOUBLE PRECISION,
    -- Metadata
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_snapshots_n_time ON leaderboard_snapshots(n, snapshot_at);
