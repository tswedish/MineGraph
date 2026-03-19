-- Fix leaderboard PK: use (n, cid) instead of (n, rank) to avoid
-- duplicate rank conflicts during concurrent admission.

-- Drop old table and recreate (safe since this is early development)
DROP TABLE IF EXISTS leaderboard;

CREATE TABLE leaderboard (
    n            INTEGER NOT NULL,
    cid          TEXT NOT NULL REFERENCES graphs(cid),
    key_id       TEXT NOT NULL REFERENCES identities(key_id),
    score_bytes  BYTEA NOT NULL,
    rank         INTEGER NOT NULL DEFAULT 0,
    admitted_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (n, cid)
);

CREATE INDEX IF NOT EXISTS idx_leaderboard_n_score ON leaderboard(n, score_bytes);
CREATE INDEX IF NOT EXISTS idx_leaderboard_cid ON leaderboard(cid);
