//! Local SQLite ledger for RamseyNet transactions and state.
//!
//! Provides storage and retrieval for challenges, graph submissions,
//! verification receipts, and derived best-known records.

pub mod error;
pub mod models;
mod queries;
mod schema;

pub use error::LedgerError;
pub use models::*;

use rusqlite::Connection;
use std::sync::Mutex;

pub const LEDGER_VERSION: &str = "0.1.0";

/// SQLite-backed ledger for the RamseyNet protocol.
///
/// Thread-safe via `Mutex<Connection>`. Each query acquires the lock,
/// executes, and releases it promptly.
pub struct Ledger {
    conn: Mutex<Connection>,
}

impl Ledger {
    /// Open (or create) a SQLite database at the given path and initialize the schema.
    pub fn open(path: &str) -> Result<Self, LedgerError> {
        let conn = Connection::open(path)?;
        schema::init_db(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self, LedgerError> {
        let conn = Connection::open_in_memory()?;
        schema::init_db(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_exists() {
        assert!(!LEDGER_VERSION.is_empty());
    }

    #[test]
    fn open_in_memory_initializes_schema() {
        let ledger = Ledger::open_in_memory().unwrap();
        let conn = ledger.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='challenges'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn challenge_crud() {
        let ledger = Ledger::open_in_memory().unwrap();

        let c = ledger.create_challenge(3, 3, "R(3,3)").unwrap();
        assert_eq!(c.challenge_id, "ramsey:3:3:v1");
        assert_eq!(c.k, 3);

        // Duplicate fails
        assert!(ledger.create_challenge(3, 3, "dup").is_err());

        // Get
        let got = ledger.get_challenge("ramsey:3:3:v1").unwrap();
        assert_eq!(got.challenge_id, "ramsey:3:3:v1");

        // Not found
        assert!(ledger.get_challenge("ramsey:99:99:v1").is_err());

        // List
        ledger.create_challenge(4, 4, "R(4,4)").unwrap();
        assert_eq!(ledger.list_challenges().unwrap().len(), 2);
    }

    #[test]
    fn submission_receipt_record_lifecycle() {
        let ledger = Ledger::open_in_memory().unwrap();
        ledger.create_challenge(3, 3, "R(3,3)").unwrap();

        // Store submission
        let sub = ledger
            .store_submission("ramsey:3:3:v1", "cid_abc", 5, r#"{"n":5}"#)
            .unwrap();
        assert_eq!(sub.graph_cid, "cid_abc");

        // Duplicate fails
        assert!(ledger
            .store_submission("ramsey:3:3:v1", "cid_abc", 5, r#"{"n":5}"#)
            .is_err());

        // Store receipt
        let receipt = ledger
            .store_receipt("cid_abc", "ramsey:3:3:v1", "accepted", None, None)
            .unwrap();
        assert_eq!(receipt.verdict, "accepted");

        // First record — new
        assert!(ledger
            .update_record_if_better("ramsey:3:3:v1", 5, "cid_abc")
            .unwrap());
        // Same n — not new
        assert!(!ledger
            .update_record_if_better("ramsey:3:3:v1", 5, "cid_abc2")
            .unwrap());
        // Better n — new
        ledger
            .store_submission("ramsey:3:3:v1", "cid_def", 6, r#"{"n":6}"#)
            .unwrap();
        assert!(ledger
            .update_record_if_better("ramsey:3:3:v1", 6, "cid_def")
            .unwrap());

        let records = ledger.list_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].best_n, 6);

        assert!(ledger.get_record("ramsey:3:3:v1").unwrap().is_some());
        assert!(ledger.get_record("ramsey:99:99:v1").unwrap().is_none());
    }

    #[test]
    fn event_log() {
        let ledger = Ledger::open_in_memory().unwrap();

        let e1 = ledger
            .append_event("challenge.created", &serde_json::json!({"k": 3}))
            .unwrap();
        assert_eq!(e1.seq, 1);

        let e2 = ledger
            .append_event("graph.submitted", &serde_json::json!({"cid": "abc"}))
            .unwrap();
        assert_eq!(e2.seq, 2);

        ledger
            .append_event("graph.verified", &serde_json::json!({"v": "ok"}))
            .unwrap();

        assert_eq!(ledger.list_events_since(0, 100).unwrap().len(), 3);
        assert_eq!(ledger.list_events_since(1, 100).unwrap().len(), 2);
        assert_eq!(ledger.list_events_since(0, 1).unwrap().len(), 1);
    }
}
