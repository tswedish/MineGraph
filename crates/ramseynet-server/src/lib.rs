use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use ramseynet_graph::{compute_cid, rgxf, RgxfJson};
use ramseynet_ledger::{Event, Ledger, LedgerError};
use ramseynet_verifier::{verify_ramsey, VerifyRequest, VerifyResponse};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

// ── Application state ────────────────────────────────────────────────

/// Shared application state threaded through all handlers.
pub struct AppState {
    pub ledger: Arc<Ledger>,
    pub event_tx: broadcast::Sender<Event>,
}

impl AppState {
    /// Store an event in the ledger and broadcast it to WebSocket subscribers.
    pub fn emit_event(
        &self,
        event_type: &str,
        payload: Value,
    ) -> Result<Event, LedgerError> {
        let event = self.ledger.append_event(event_type, &payload)?;
        // Best-effort broadcast — ignore error if no receivers
        let _ = self.event_tx.send(event.clone());
        Ok(event)
    }
}

// ── Error mapping ────────────────────────────────────────────────────

type ApiError = (StatusCode, Json<Value>);

fn map_ledger_error(e: LedgerError) -> ApiError {
    match &e {
        LedgerError::ChallengeNotFound(_) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": e.to_string() })))
        }
        LedgerError::ChallengeAlreadyExists(_) => {
            (StatusCode::CONFLICT, Json(json!({ "error": e.to_string() })))
        }
        LedgerError::GraphNotFound(_) => {
            (StatusCode::NOT_FOUND, Json(json!({ "error": e.to_string() })))
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

// ── Handlers ─────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({
        "name": "RamseyNet",
        "version": ramseynet_types::PROTOCOL_VERSION,
        "status": "ok"
    }))
}

async fn list_challenges(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    let ledger = state.ledger.clone();
    let challenges = tokio::task::spawn_blocking(move || ledger.list_challenges())
        .await
        .unwrap()
        .map_err(map_ledger_error)?;
    Ok(Json(json!({ "challenges": challenges })))
}

#[derive(Deserialize)]
struct CreateChallengeRequest {
    k: u32,
    ell: u32,
    #[serde(default)]
    description: String,
}

async fn create_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateChallengeRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let ledger = state.ledger.clone();
    let k = req.k;
    let ell = req.ell;
    let desc = req.description.clone();
    let challenge = tokio::task::spawn_blocking(move || ledger.create_challenge(k, ell, &desc))
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

    // Emit event
    let _ = state.emit_event(
        "challenge.created",
        json!({
            "challenge_id": challenge.challenge_id,
            "k": challenge.k,
            "ell": challenge.ell,
            "description": challenge.description,
        }),
    );

    Ok((StatusCode::CREATED, Json(json!({ "challenge": challenge }))))
}

async fn get_challenge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let ledger = state.ledger.clone();
    let id2 = id.clone();
    let challenge = tokio::task::spawn_blocking(move || ledger.get_challenge(&id2))
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

    let ledger2 = state.ledger.clone();
    let id3 = id.clone();
    let record = tokio::task::spawn_blocking(move || ledger2.get_record(&id3))
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

    Ok(Json(json!({
        "challenge": challenge,
        "record": record,
    })))
}

async fn list_records(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    let ledger = state.ledger.clone();
    let records = tokio::task::spawn_blocking(move || ledger.list_records())
        .await
        .unwrap()
        .map_err(map_ledger_error)?;
    Ok(Json(json!({ "records": records })))
}

/// Stateless verification — no database interaction.
async fn verify(
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let adj = rgxf::from_json(&req.graph).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid RGXF: {e}") })),
        )
    })?;

    let cid = compute_cid(&adj);
    let result = verify_ramsey(&adj, req.k, req.ell, &cid);

    let mut response: VerifyResponse = result.into();
    if !req.want_cid {
        response.graph_cid = None;
    }

    Ok(Json(response))
}

#[derive(Deserialize)]
struct SubmitRequest {
    challenge_id: String,
    graph: RgxfJson,
}

/// Full lifecycle: verify + store + update records + emit events.
async fn submit_graph(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    // 1. Validate challenge exists
    let ledger = state.ledger.clone();
    let cid_str = req.challenge_id.clone();
    tokio::task::spawn_blocking(move || ledger.get_challenge(&cid_str))
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

    // 2. Decode RGXF and verify
    let adj = rgxf::from_json(&req.graph).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid RGXF: {e}") })),
        )
    })?;
    let n = adj.n();
    let cid = compute_cid(&adj);
    let cid_hex = cid.to_hex();

    // Parse k, ell from challenge_id (format: "ramsey:{k}:{ell}:v1")
    let parts: Vec<&str> = req.challenge_id.split(':').collect();
    let (k, ell) = if parts.len() >= 3 {
        let k: u32 = parts[1].parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid challenge_id format" })),
            )
        })?;
        let ell: u32 = parts[2].parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Invalid challenge_id format" })),
            )
        })?;
        (k, ell)
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid challenge_id format" })),
        ));
    };

    let result = verify_ramsey(&adj, k, ell, &cid);

    // 3. Store submission (handle duplicates gracefully)
    let rgxf_json_str = serde_json::to_string(&req.graph).unwrap();
    let ledger = state.ledger.clone();
    let challenge_id = req.challenge_id.clone();
    let cid_hex2 = cid_hex.clone();
    let is_duplicate = {
        let result = tokio::task::spawn_blocking(move || {
            ledger.store_submission(&challenge_id, &cid_hex2, n, &rgxf_json_str)
        })
        .await
        .unwrap();
        match result {
            Ok(_) => false,
            Err(LedgerError::GraphAlreadySubmitted(_)) => true,
            Err(e) => return Err(map_ledger_error(e)),
        }
    };

    // Emit graph.submitted event (only for new submissions)
    if !is_duplicate {
        let _ = state.emit_event(
            "graph.submitted",
            json!({
                "graph_cid": cid_hex,
                "challenge_id": req.challenge_id,
                "n": n,
            }),
        );
    }

    // 4. Store verification receipt (skip if duplicate — already verified)
    let verdict_str = result.verdict.to_string();
    let reason = result.reason.clone();
    let witness = result.witness.clone();

    if !is_duplicate {
        let ledger = state.ledger.clone();
        let cid_hex3 = cid_hex.clone();
        let challenge_id2 = req.challenge_id.clone();
        let verdict2 = verdict_str.clone();
        let reason2 = reason.clone();
        let witness2 = witness.clone();
        tokio::task::spawn_blocking(move || {
            ledger.store_receipt(
                &cid_hex3,
                &challenge_id2,
                &verdict2,
                reason2.as_deref(),
                witness2.as_deref(),
            )
        })
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

        // Emit graph.verified event
        let _ = state.emit_event(
            "graph.verified",
            json!({
                "graph_cid": cid_hex,
                "challenge_id": req.challenge_id,
                "verdict": verdict_str,
                "reason": reason,
                "witness": witness,
            }),
        );
    }

    // 5. Update record if accepted and better
    let mut is_new_record = false;
    if verdict_str == "accepted" && !is_duplicate {
        let ledger = state.ledger.clone();
        let challenge_id3 = req.challenge_id.clone();
        let cid_hex4 = cid_hex.clone();
        is_new_record = tokio::task::spawn_blocking(move || {
            ledger.update_record_if_better(&challenge_id3, n, &cid_hex4)
        })
        .await
        .unwrap()
        .map_err(map_ledger_error)?;

        if is_new_record {
            let _ = state.emit_event(
                "record.updated",
                json!({
                    "challenge_id": req.challenge_id,
                    "best_n": n,
                    "best_cid": cid_hex,
                }),
            );
        }
    }

    let status_code = if is_duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((
        status_code,
        Json(json!({
            "graph_cid": cid_hex,
            "verdict": verdict_str,
            "reason": reason,
            "witness": witness,
            "is_new_record": is_new_record,
        })),
    ))
}

/// OESP-1 WebSocket event stream.
async fn ws_events(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<AppState>) {
    // Wait for optional initial message with replay request
    let mut after_seq: i64 = 0;

    // Try to read an initial message (with timeout)
    let initial = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        socket.recv(),
    )
    .await;

    if let Ok(Some(Ok(Message::Text(text)))) = initial {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(seq) = v.get("after_seq").and_then(|s| s.as_i64()) {
                after_seq = seq;
            }
        }
    }

    // Replay missed events from DB
    if after_seq >= 0 {
        let ledger = state.ledger.clone();
        let seq = after_seq;
        if let Ok(events) =
            tokio::task::spawn_blocking(move || ledger.list_events_since(seq, 1000))
                .await
                .unwrap()
        {
            for event in events {
                let msg = serde_json::to_string(&event).unwrap();
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    return;
                }
            }
        }
    }

    // Subscribe to live events
    let mut rx = state.event_tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                let msg = serde_json::to_string(&event).unwrap();
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

// ── Router ───────────────────────────────────────────────────────────

/// Create the application router with shared state.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(health))
        .route("/api/health", get(health))
        .route(
            "/api/challenges",
            get(list_challenges).post(create_challenge),
        )
        .route("/api/challenges/{id}", get(get_challenge))
        .route("/api/records", get(list_records))
        .route("/api/verify", post(verify))
        .route("/api/submit", post(submit_graph))
        .route("/api/events", get(ws_events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
