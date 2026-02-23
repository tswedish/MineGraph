use chrono::Utc;
use rusqlite::params;

use crate::error::LedgerError;
use crate::models::*;
use crate::Ledger;

// ── Challenge operations ─────────────────────────────────────────────

impl Ledger {
    /// Create a new challenge. Returns `ChallengeAlreadyExists` if the ID is taken.
    pub fn create_challenge(
        &self,
        k: u32,
        ell: u32,
        description: &str,
    ) -> Result<Challenge, LedgerError> {
        let challenge_id = format!("ramsey:{k}:{ell}:v1");
        let now = Utc::now();
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            "INSERT INTO challenges (challenge_id, k, ell, description, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![challenge_id, k, ell, description, now.to_rfc3339()],
        );
        match result {
            Ok(_) => Ok(Challenge {
                challenge_id,
                k,
                ell,
                description: description.to_string(),
                created_at: now,
            }),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(LedgerError::ChallengeAlreadyExists(challenge_id))
            }
            Err(e) => Err(LedgerError::Db(e)),
        }
    }

    /// Get a challenge by ID.
    pub fn get_challenge(&self, id: &str) -> Result<Challenge, LedgerError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT challenge_id, k, ell, description, created_at FROM challenges WHERE challenge_id = ?1",
            params![id],
            |row| {
                Ok(Challenge {
                    challenge_id: row.get(0)?,
                    k: row.get(1)?,
                    ell: row.get(2)?,
                    description: row.get(3)?,
                    created_at: parse_datetime(row.get::<_, String>(4)?),
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => LedgerError::ChallengeNotFound(id.to_string()),
            other => LedgerError::Db(other),
        })
    }

    /// List all challenges, ordered by creation time.
    pub fn list_challenges(&self) -> Result<Vec<Challenge>, LedgerError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT challenge_id, k, ell, description, created_at FROM challenges ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Challenge {
                challenge_id: row.get(0)?,
                k: row.get(1)?,
                ell: row.get(2)?,
                description: row.get(3)?,
                created_at: parse_datetime(row.get::<_, String>(4)?),
            })
        })?;
        let mut challenges = Vec::new();
        for row in rows {
            challenges.push(row?);
        }
        Ok(challenges)
    }
}

// ── Submission + Receipt operations ──────────────────────────────────

impl Ledger {
    /// Store a graph submission. Returns `GraphAlreadySubmitted` if the CID exists.
    pub fn store_submission(
        &self,
        challenge_id: &str,
        graph_cid: &str,
        n: u32,
        rgxf_json: &str,
    ) -> Result<Submission, LedgerError> {
        let now = Utc::now();
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            "INSERT INTO graph_submissions (graph_cid, challenge_id, n, rgxf_json, submitted_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![graph_cid, challenge_id, n, rgxf_json, now.to_rfc3339()],
        );
        match result {
            Ok(_) => Ok(Submission {
                graph_cid: graph_cid.to_string(),
                challenge_id: challenge_id.to_string(),
                n,
                rgxf_json: rgxf_json.to_string(),
                submitted_at: now,
            }),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(LedgerError::GraphAlreadySubmitted(graph_cid.to_string()))
            }
            Err(e) => Err(LedgerError::Db(e)),
        }
    }

    /// Store a verification receipt.
    pub fn store_receipt(
        &self,
        graph_cid: &str,
        challenge_id: &str,
        verdict: &str,
        reason: Option<&str>,
        witness: Option<&[u32]>,
    ) -> Result<Receipt, LedgerError> {
        let now = Utc::now();
        let witness_json = witness.map(|w| serde_json::to_string(w).unwrap());
        let conn = self.conn.lock().unwrap();
        let receipt_id = conn.query_row(
            "INSERT INTO verify_receipts (graph_cid, challenge_id, verdict, reason, witness_json, verified_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING receipt_id",
            params![graph_cid, challenge_id, verdict, reason, witness_json, now.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(Receipt {
            receipt_id,
            graph_cid: graph_cid.to_string(),
            challenge_id: challenge_id.to_string(),
            verdict: verdict.to_string(),
            reason: reason.map(|s| s.to_string()),
            witness: witness.map(|w| w.to_vec()),
            verified_at: now,
        })
    }
}

// ── Record operations ────────────────────────────────────────────────

impl Ledger {
    /// Update the best record for a challenge if this graph is better.
    /// Returns `true` if a new record was set.
    pub fn update_record_if_better(
        &self,
        challenge_id: &str,
        n: u32,
        graph_cid: &str,
    ) -> Result<bool, LedgerError> {
        let now = Utc::now();
        let conn = self.conn.lock().unwrap();

        // Check current best
        let current_best: Option<u32> = conn
            .query_row(
                "SELECT best_n FROM records WHERE challenge_id = ?1",
                params![challenge_id],
                |row| row.get(0),
            )
            .ok();

        let should_update = match current_best {
            None => true,
            Some(best) => n > best,
        };

        if should_update {
            conn.execute(
                "INSERT INTO records (challenge_id, best_n, best_cid, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(challenge_id) DO UPDATE SET best_n = ?2, best_cid = ?3, updated_at = ?4",
                params![challenge_id, n, graph_cid, now.to_rfc3339()],
            )?;
        }

        Ok(should_update)
    }

    /// Get the record for a specific challenge.
    pub fn get_record(&self, challenge_id: &str) -> Result<Option<Record>, LedgerError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT challenge_id, best_n, best_cid, updated_at FROM records WHERE challenge_id = ?1",
            params![challenge_id],
            |row| {
                Ok(Record {
                    challenge_id: row.get(0)?,
                    best_n: row.get(1)?,
                    best_cid: row.get(2)?,
                    updated_at: parse_datetime(row.get::<_, String>(3)?),
                })
            },
        )
        .optional()
        .map_err(LedgerError::Db)
    }

    /// List all records, ordered by challenge ID.
    pub fn list_records(&self) -> Result<Vec<Record>, LedgerError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT challenge_id, best_n, best_cid, updated_at FROM records ORDER BY challenge_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Record {
                challenge_id: row.get(0)?,
                best_n: row.get(1)?,
                best_cid: row.get(2)?,
                updated_at: parse_datetime(row.get::<_, String>(3)?),
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

// ── Submission queries ───────────────────────────────────────────────

impl Ledger {
    /// Get the RGXF JSON string for a submission by CID.
    pub fn get_submission_rgxf(&self, cid: &str) -> Result<Option<String>, LedgerError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT rgxf_json FROM graph_submissions WHERE graph_cid = ?1",
            params![cid],
            |row| row.get(0),
        )
        .optional()
        .map_err(LedgerError::Db)
    }

    /// Get full submission detail by CID: submission + optional receipt + optional challenge.
    #[allow(clippy::type_complexity)]
    pub fn get_submission_detail(
        &self,
        cid: &str,
    ) -> Result<Option<(Submission, Option<Receipt>, Option<Challenge>)>, LedgerError> {
        let conn = self.conn.lock().unwrap();

        // 1. Get submission
        let submission: Option<Submission> = conn
            .query_row(
                "SELECT graph_cid, challenge_id, n, rgxf_json, submitted_at FROM graph_submissions WHERE graph_cid = ?1",
                params![cid],
                |row| {
                    Ok(Submission {
                        graph_cid: row.get(0)?,
                        challenge_id: row.get(1)?,
                        n: row.get(2)?,
                        rgxf_json: row.get(3)?,
                        submitted_at: parse_datetime(row.get::<_, String>(4)?),
                    })
                },
            )
            .optional()
            .map_err(LedgerError::Db)?;

        let submission = match submission {
            Some(s) => s,
            None => return Ok(None),
        };

        // 2. Get receipt (optional — may not be verified yet)
        let receipt: Option<Receipt> = conn
            .query_row(
                "SELECT receipt_id, graph_cid, challenge_id, verdict, reason, witness_json, verified_at FROM verify_receipts WHERE graph_cid = ?1",
                params![cid],
                |row| {
                    let witness_str: Option<String> = row.get(5)?;
                    let witness: Option<Vec<u32>> =
                        witness_str.and_then(|s| serde_json::from_str(&s).ok());
                    Ok(Receipt {
                        receipt_id: row.get(0)?,
                        graph_cid: row.get(1)?,
                        challenge_id: row.get(2)?,
                        verdict: row.get(3)?,
                        reason: row.get(4)?,
                        witness,
                        verified_at: parse_datetime(row.get::<_, String>(6)?),
                    })
                },
            )
            .optional()
            .map_err(LedgerError::Db)?;

        // 3. Get challenge context
        let challenge: Option<Challenge> = conn
            .query_row(
                "SELECT challenge_id, k, ell, description, created_at FROM challenges WHERE challenge_id = ?1",
                params![submission.challenge_id],
                |row| {
                    Ok(Challenge {
                        challenge_id: row.get(0)?,
                        k: row.get(1)?,
                        ell: row.get(2)?,
                        description: row.get(3)?,
                        created_at: parse_datetime(row.get::<_, String>(4)?),
                    })
                },
            )
            .optional()
            .map_err(LedgerError::Db)?;

        Ok(Some((submission, receipt, challenge)))
    }
}

// ── Event operations ─────────────────────────────────────────────────

impl Ledger {
    /// Append an event to the log and return it with its assigned sequence number.
    pub fn append_event(
        &self,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> Result<Event, LedgerError> {
        let now = Utc::now();
        let payload_json = serde_json::to_string(payload)?;
        let conn = self.conn.lock().unwrap();
        let seq: i64 = conn.query_row(
            "INSERT INTO events (event_type, payload_json, created_at) VALUES (?1, ?2, ?3) RETURNING seq",
            params![event_type, payload_json, now.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(Event {
            seq,
            event_type: event_type.to_string(),
            payload: payload.clone(),
            created_at: now,
        })
    }

    /// List events with sequence number > `after_seq`, up to `limit`.
    pub fn list_events_since(
        &self,
        after_seq: i64,
        limit: u32,
    ) -> Result<Vec<Event>, LedgerError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, event_type, payload_json, created_at FROM events WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after_seq, limit], |row| {
            let payload_str: String = row.get(2)?;
            Ok(Event {
                seq: row.get(0)?,
                event_type: row.get(1)?,
                payload: serde_json::from_str(&payload_str).unwrap_or_default(),
                created_at: parse_datetime(row.get::<_, String>(3)?),
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Helper trait to convert `QueryReturnedNoRows` into `None`.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Parse an ISO 8601 datetime string. Falls back to epoch on parse error.
fn parse_datetime(s: String) -> chrono::DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default()
}
