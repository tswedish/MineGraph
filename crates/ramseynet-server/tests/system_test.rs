//! System functional test for the RamseyNet server.
//!
//! Spins up a real Axum server on a random port and exercises the full
//! HTTP API with realistic Ramsey graph payloads. This verifies:
//! - Server boots and serves all endpoints
//! - Graph encoding (RGXF) works end-to-end over HTTP
//! - Verifier correctly accepts/rejects graphs
//! - CID computation is stable across requests
//! - Error handling returns proper 400 responses

use ramseynet_graph::{rgxf, AdjacencyMatrix};
use ramseynet_verifier::VerifyRequest;
use serde_json::Value;

/// Start a real server on port 0 (OS-assigned) and return the base URL.
async fn start_server() -> (String, tokio::task::JoinHandle<()>) {
    let app = ramseynet_server::create_router();
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
        eprintln!("[PASS] Health check: {}", body);
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
        eprintln!("[PASS] Challenges list is empty");
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
        eprintln!("[PASS] Records list is empty");
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
        eprintln!("[PASS] C5 accepted for R(3,3), CID: {}", &c5_cid[..16]);
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
        eprintln!("[PASS] K5 rejected for R(3,3), witness: {:?}", witness);
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
            "[PASS] Empty graph rejected for R(3,3), witness: {:?}",
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
        eprintln!("[PASS] Invalid RGXF returns 400: {}", body["error"]);
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
        eprintln!("[PASS] CID stability: both C5 submissions → {}", &c5_cid[..16]);
    }

    eprintln!("\n✓ All 8 system tests passed!");
}
