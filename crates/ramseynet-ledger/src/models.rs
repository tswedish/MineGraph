use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Ramsey challenge arena.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Challenge {
    pub challenge_id: String,
    pub k: u32,
    pub ell: u32,
    pub description: String,
    pub created_at: DateTime<Utc>,
}

/// A graph submission linked to a challenge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Submission {
    pub graph_cid: String,
    pub challenge_id: String,
    pub n: u32,
    pub rgxf_json: String,
    pub submitted_at: DateTime<Utc>,
}

/// A verification receipt for a submitted graph.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_id: i64,
    pub graph_cid: String,
    pub challenge_id: String,
    pub verdict: String,
    pub reason: Option<String>,
    pub witness: Option<Vec<u32>>,
    pub verified_at: DateTime<Utc>,
}

/// Best-known record for a challenge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub challenge_id: String,
    pub best_n: u32,
    pub best_cid: String,
    pub updated_at: DateTime<Utc>,
}

/// An event in the OESP-1 event log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}
