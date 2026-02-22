//! System functional test for the RamseyNet server.
//!
//! Spins up a real Axum server on a random port and exercises the full
//! HTTP API with realistic Ramsey graph payloads. This verifies:
//! - Server boots and serves all endpoints
//! - Graph encoding (RGXF) works end-to-end over HTTP
//! - Verifier correctly accepts/rejects graphs
//! - CID computation is stable across requests
//! - Error handling returns proper 400 responses
//! - Challenge CRUD (create, get, list)
//! - Submit lifecycle (verify + store + record update + events)
//! - WebSocket event streaming (OESP-1)

use std::sync::Arc;

use ramseynet_graph::{rgxf, AdjacencyMatrix};
use ramseynet_ledger::Ledger;
use ramseynet_server::AppState;
use ramseynet_verifier::VerifyRequest;
use serde_json::Value;

/// Start a real server on port 0 (OS-assigned) with an in-memory ledger.
async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let ledger = Arc::new(Ledger::open_in_memory().expect("in-memory ledger"));
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let state = Arc::new(AppState { ledger, event_tx });

    let app = ramseynet_server::create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base_url = format!("http://127.0.0.1:{port}");

    // Give the server a moment to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, handle)
}

/// Build a C5 (5-cycle) graph — the classic R(3,3) witness on 5 vertices.
/// Has omega=2, alpha=2 so it should be ACCEPTED for R(3,3).
fn build_c5() -> ramseynet_graph::RgxfJson {
    let mut g = AdjacencyMatrix::new(5);
    g.set_edge(0, 1, true);
    g.set_edge(1, 2, true);
    g.set_edge(2, 3, true);
    g.set_edge(3, 4, true);
    g.set_edge(4, 0, true);
    rgxf::to_json(&g)
}

/// Build K_n (complete graph on n vertices).
/// For n >= 3, has a 3-clique so should be REJECTED for R(3,3).
fn build_kn(n: u32) -> ramseynet_graph::RgxfJson {
    let mut g = AdjacencyMatrix::new(n);
    for i in 0..n {
        for j in (i + 1)..n {
            g.set_edge(i, j, true);
        }
    }
    rgxf::to_json(&g)
}

/// Build an empty graph on n vertices (no edges).
/// For n >= 3, has a 3-independent-set so should be REJECTED for R(3,3).
fn build_empty(n: u32) -> ramseynet_graph::RgxfJson {
    let g = AdjacencyMatrix::new(n);
    rgxf::to_json(&g)
}

fn make_verify_request(
    k: u32,
    ell: u32,
    graph: ramseynet_graph::RgxfJson,
    want_cid: bool,
) -> VerifyRequest {
    VerifyRequest {
        oras_version: "ovwc-1".to_string(),
        k,
        ell,
        graph,
        want_cid,
    }
}

// ── Test: Full Ramsey verification lifecycle ──────────────────────────

#[tokio::test]
async fn system_test_full_lifecycle() {
    let (base, _handle) = start_server().await;
    let client = reqwest::Client::new();

    // ── 1. Health check ──────────────────────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/health"))
            .send()
            .await
            .expect("health request failed");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["name"], "RamseyNet");
        assert_eq!(body["status"], "ok");
        assert!(body["version"].is_string());
        eprintln!("[PASS] 1. Health check: {}", body);
    }

    // ── 2. List challenges (empty) ───────────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/challenges"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["challenges"], Value::Array(vec![]));
        eprintln!("[PASS] 2. Challenges list is empty");
    }

    // ── 3. List records (empty) ──────────────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/records"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["records"], Value::Array(vec![]));
        eprintln!("[PASS] 3. Records list is empty");
    }

    // ── 4. Verify C5 for R(3,3) — should be ACCEPTED ────────────────
    let c5_cid: String;
    {
        let req = make_verify_request(3, 3, build_c5(), true);
        let resp = client
            .post(format!("{base}/api/verify"))
            .json(&req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "accepted");
        assert!(body["reason"].is_null());
        assert!(body["witness"].is_null());
        let cid = body["graph_cid"].as_str().unwrap();
        assert!(!cid.is_empty());
        assert_eq!(cid.len(), 64); // SHA-256 hex = 64 chars
        c5_cid = cid.to_string();
        eprintln!("[PASS] 4. C5 accepted for R(3,3), CID: {}", &c5_cid[..16]);
    }

    // ── 5. Verify K5 for R(3,3) — should be REJECTED (clique) ───────
    {
        let req = make_verify_request(3, 3, build_kn(5), true);
        let resp = client
            .post(format!("{base}/api/verify"))
            .json(&req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "rejected");
        assert_eq!(body["reason"], "clique_found");
        let witness: Vec<u32> = serde_json::from_value(body["witness"].clone()).unwrap();
        assert_eq!(witness, vec![0, 1, 2]);
        eprintln!("[PASS] 5. K5 rejected for R(3,3), witness: {:?}", witness);
    }

    // ── 6. Verify empty graph for R(3,3) — REJECTED (independent set) ─
    {
        let req = make_verify_request(3, 3, build_empty(5), true);
        let resp = client
            .post(format!("{base}/api/verify"))
            .json(&req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "rejected");
        assert_eq!(body["reason"], "independent_set_found");
        let witness: Vec<u32> = serde_json::from_value(body["witness"].clone()).unwrap();
        assert_eq!(witness, vec![0, 1, 2]);
        eprintln!(
            "[PASS] 6. Empty graph rejected for R(3,3), witness: {:?}",
            witness
        );
    }

    // ── 7. Invalid RGXF — should get 400 ────────────────────────────
    {
        let bad_graph = ramseynet_graph::RgxfJson {
            n: 5,
            encoding: "utri_b64_v1".to_string(),
            // Wrong length: n=5 needs ceil(10/8) = 2 bytes, but "/w==" is only 1 byte
            bits_b64: "/w==".to_string(),
        };
        let req = make_verify_request(3, 3, bad_graph, false);
        let resp = client
            .post(format!("{base}/api/verify"))
            .json(&req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
        let body: Value = resp.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("Invalid RGXF"));
        eprintln!("[PASS] 7. Invalid RGXF returns 400: {}", body["error"]);
    }

    // ── 8. CID stability — same graph → same CID ────────────────────
    {
        let req = make_verify_request(3, 3, build_c5(), true);
        let resp = client
            .post(format!("{base}/api/verify"))
            .json(&req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let cid2 = body["graph_cid"].as_str().unwrap();
        assert_eq!(cid2, c5_cid, "CID must be deterministic");
        eprintln!(
            "[PASS] 8. CID stability: both C5 submissions → {}",
            &c5_cid[..16]
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Phase 3: Challenge + Submit lifecycle tests
    // ══════════════════════════════════════════════════════════════════

    // ── 9. Create challenge R(3,3) ───────────────────────────────────
    {
        let resp = client
            .post(format!("{base}/api/challenges"))
            .json(&serde_json::json!({
                "k": 3,
                "ell": 3,
                "description": "Find R(3,3) witnesses"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["challenge"]["challenge_id"], "ramsey:3:3:v1");
        assert_eq!(body["challenge"]["k"], 3);
        assert_eq!(body["challenge"]["ell"], 3);
        eprintln!("[PASS] 9. Created challenge R(3,3)");
    }

    // ── 10. Duplicate challenge → 409 ────────────────────────────────
    {
        let resp = client
            .post(format!("{base}/api/challenges"))
            .json(&serde_json::json!({
                "k": 3,
                "ell": 3,
                "description": "duplicate"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 409);
        let body: Value = resp.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("already exists"));
        eprintln!("[PASS] 10. Duplicate challenge returns 409");
    }

    // ── 11. Create a second challenge R(4,4) ─────────────────────────
    {
        let resp = client
            .post(format!("{base}/api/challenges"))
            .json(&serde_json::json!({
                "k": 4,
                "ell": 4,
                "description": "Find R(4,4) witnesses"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["challenge"]["challenge_id"], "ramsey:4:4:v1");
        eprintln!("[PASS] 11. Created challenge R(4,4)");
    }

    // ── 12. List challenges (should have 2) ──────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/challenges"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let challenges = body["challenges"].as_array().unwrap();
        assert_eq!(challenges.len(), 2);
        eprintln!("[PASS] 12. List challenges returns 2");
    }

    // ── 13. Submit C5 to R(3,3) — accepted, new record ──────────────
    {
        let resp = client
            .post(format!("{base}/api/submit"))
            .json(&serde_json::json!({
                "challenge_id": "ramsey:3:3:v1",
                "graph": build_c5(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["verdict"], "accepted");
        assert_eq!(body["graph_cid"], c5_cid);
        assert_eq!(body["is_new_record"], true);
        assert!(body["reason"].is_null());
        assert!(body["witness"].is_null());
        eprintln!(
            "[PASS] 13. Submit C5 to R(3,3): accepted, new record, CID: {}",
            &c5_cid[..16]
        );
    }

    // ── 14. Submit K5 to R(3,3) — rejected, no record ───────────────
    {
        let resp = client
            .post(format!("{base}/api/submit"))
            .json(&serde_json::json!({
                "challenge_id": "ramsey:3:3:v1",
                "graph": build_kn(5),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["verdict"], "rejected");
        assert_eq!(body["reason"], "clique_found");
        assert_eq!(body["is_new_record"], false);
        eprintln!("[PASS] 14. Submit K5 to R(3,3): rejected, not a record");
    }

    // ── 15. Duplicate submission (C5 again) → 200 (not 201) ─────────
    {
        let resp = client
            .post(format!("{base}/api/submit"))
            .json(&serde_json::json!({
                "challenge_id": "ramsey:3:3:v1",
                "graph": build_c5(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "duplicate submission should return 200");
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["verdict"], "accepted");
        assert_eq!(body["is_new_record"], false, "duplicate cannot be a new record");
        eprintln!("[PASS] 15. Duplicate C5 submission returns 200");
    }

    // ── 16. List records (should have 1 for R(3,3)) ──────────────────
    {
        let resp = client
            .get(format!("{base}/api/records"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let records = body["records"].as_array().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["challenge_id"], "ramsey:3:3:v1");
        assert_eq!(records[0]["best_n"], 5);
        assert_eq!(records[0]["best_cid"], c5_cid);
        eprintln!("[PASS] 16. Records list has 1 entry with best_n=5");
    }

    // ── 17. Get challenge detail includes record ─────────────────────
    {
        let resp = client
            .get(format!("{base}/api/challenges/ramsey:3:3:v1"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["challenge"]["challenge_id"], "ramsey:3:3:v1");
        assert_eq!(body["record"]["best_n"], 5);
        assert_eq!(body["record"]["best_cid"], c5_cid);
        eprintln!("[PASS] 17. Challenge detail includes record");
    }

    // ── 18. Submit to non-existent challenge → 404 ───────────────────
    {
        let resp = client
            .post(format!("{base}/api/submit"))
            .json(&serde_json::json!({
                "challenge_id": "ramsey:99:99:v1",
                "graph": build_c5(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
        let body: Value = resp.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("not found"));
        eprintln!("[PASS] 18. Submit to missing challenge returns 404");
    }

    // ── 19. Get non-existent challenge → 404 ─────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/challenges/ramsey:99:99:v1"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
        eprintln!("[PASS] 19. Get missing challenge returns 404");
    }

    eprintln!("\n✓ All 19 system tests passed!");
}

// ── WebSocket event stream test ──────────────────────────────────────

#[tokio::test]
async fn system_test_websocket_events() {
    use futures_util::StreamExt;
    use tokio_tungstenite::connect_async;

    let (base, _handle) = start_server().await;
    let client = reqwest::Client::new();
    let ws_url = base.replace("http://", "ws://") + "/api/events";

    // Connect WebSocket
    let (mut ws, _resp) = connect_async(&ws_url).await.expect("WS connect failed");

    // Give WS a moment to establish
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Create a challenge — should produce an event
    let resp = client
        .post(format!("{base}/api/challenges"))
        .json(&serde_json::json!({
            "k": 5,
            "ell": 5,
            "description": "WS test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Read the event from WebSocket
    let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
        .await
        .expect("WS timeout waiting for event")
        .expect("WS stream ended")
        .expect("WS message error");

    let text = msg.to_text().expect("not text");
    let event: Value = serde_json::from_str(text).unwrap();
    assert_eq!(event["event_type"], "challenge.created");
    assert_eq!(event["seq"], 1);

    // payload is a nested JSON value (not a string)
    let payload = &event["payload"];
    assert_eq!(payload["k"], 5);
    assert_eq!(payload["ell"], 5);
    eprintln!("[PASS] WebSocket received challenge.created event: seq={}", event["seq"]);

    // Submit a graph — should produce graph.submitted + graph.verified + record.updated
    let resp = client
        .post(format!("{base}/api/submit"))
        .json(&serde_json::json!({
            "challenge_id": "ramsey:5:5:v1",
            "graph": build_c5(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Read the submit events
    let mut event_types = Vec::new();
    for _ in 0..3 {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
            .await
            .expect("WS timeout")
            .expect("WS stream ended")
            .expect("WS error");
        let event: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
        event_types.push(event["event_type"].as_str().unwrap().to_string());
    }

    assert!(event_types.contains(&"graph.submitted".to_string()));
    assert!(event_types.contains(&"graph.verified".to_string()));
    assert!(event_types.contains(&"record.updated".to_string()));
    eprintln!("[PASS] WebSocket received submit lifecycle events: {:?}", event_types);

    // WebSocket will close when dropped
    drop(ws);
    eprintln!("\n✓ WebSocket event test passed!");
}
