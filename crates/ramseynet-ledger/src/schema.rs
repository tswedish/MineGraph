use rusqlite::Connection;

use crate::LedgerError;

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('version', '1');

CREATE TABLE IF NOT EXISTS challenges (
    challenge_id TEXT PRIMARY KEY,
    k            INTEGER NOT NULL,
    ell          INTEGER NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL,
    CHECK (k >= 2 AND ell >= 2)
);

CREATE TABLE IF NOT EXISTS graph_submissions (
    graph_cid    TEXT PRIMARY KEY,
    challenge_id TEXT NOT NULL REFERENCES challenges(challenge_id),
    n            INTEGER NOT NULL,
    rgxf_json    TEXT NOT NULL,
    submitted_at TEXT NOT NULL,
    CHECK (n >= 0)
);

CREATE TABLE IF NOT EXISTS verify_receipts (
    receipt_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    graph_cid    TEXT NOT NULL UNIQUE REFERENCES graph_submissions(graph_cid),
    challenge_id TEXT NOT NULL REFERENCES challenges(challenge_id),
    verdict      TEXT NOT NULL,
    reason       TEXT,
    witness_json TEXT,
    verified_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS records (
    challenge_id TEXT PRIMARY KEY REFERENCES challenges(challenge_id),
    best_n       INTEGER NOT NULL,
    best_cid     TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type   TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   TEXT NOT NULL
);
";

/// Initialize the database schema. Enables WAL mode for better concurrent reads.
pub fn init_db(conn: &Connection) -> Result<(), LedgerError> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}
