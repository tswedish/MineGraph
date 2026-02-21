use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use ramseynet_graph::{compute_cid, rgxf};
use ramseynet_verifier::{verify_ramsey, VerifyRequest, VerifyResponse};
use serde_json::{json, Value};
use tower_http::cors::CorsLayer;

async fn health() -> Json<Value> {
    Json(json!({
        "name": "RamseyNet",
        "version": ramseynet_types::PROTOCOL_VERSION,
        "status": "ok"
    }))
}

async fn list_challenges() -> Json<Value> {
    Json(json!({ "challenges": [] }))
}

async fn list_records() -> Json<Value> {
    Json(json!({ "records": [] }))
}

async fn verify(
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, Json<Value>)> {
    // Decode RGXF JSON into adjacency matrix
    let adj = rgxf::from_json(&req.graph).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid RGXF: {e}") })),
        )
    })?;

    // Compute content ID and verify
    let cid = compute_cid(&adj);
    let result = verify_ramsey(&adj, req.k, req.ell, &cid);

    // Convert to response
    let mut response: VerifyResponse = result.into();
    if !req.want_cid {
        response.graph_cid = None;
    }

    Ok(Json(response))
}

/// Create the application router. Extracted so integration tests can reuse it.
pub fn create_router() -> Router {
    Router::new()
        .route("/", get(health))
        .route("/api/health", get(health))
        .route("/api/challenges", get(list_challenges))
        .route("/api/records", get(list_records))
        .route("/api/verify", post(verify))
        .layer(CorsLayer::permissive())
}
