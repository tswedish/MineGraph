use ramseynet_graph::RgxfJson;
use serde::{Deserialize, Serialize};

use crate::error::SearchError;

/// Challenge info returned by the server.
#[derive(Debug, Deserialize)]
pub struct ChallengeInfo {
    pub challenge: ChallengeDetail,
    pub record: Option<RecordInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ChallengeDetail {
    pub challenge_id: String,
    pub k: u32,
    pub ell: u32,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct RecordInfo {
    pub best_n: u32,
    pub best_cid: String,
}

/// Submit response from the server.
#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    pub graph_cid: String,
    pub verdict: String,
    pub is_new_record: Option<bool>,
    pub reason: Option<String>,
    pub witness: Option<Vec<u32>>,
}

#[derive(Serialize)]
struct SubmitRequest {
    challenge_id: String,
    graph: RgxfJson,
}

/// Async HTTP client for the RamseyNet server.
pub struct ServerClient {
    base_url: String,
    client: reqwest::Client,
}

impl ServerClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Fetch challenge details and current record.
    pub async fn get_challenge(&self, challenge_id: &str) -> Result<ChallengeInfo, SearchError> {
        let url = format!("{}/api/challenges/{}", self.base_url, challenge_id);
        let resp = self.client.get(&url).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(SearchError::ChallengeNotFound(challenge_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SearchError::ServerError(format!("{status}: {body}")));
        }

        let info: ChallengeInfo = resp.json().await?;
        Ok(info)
    }

    /// Submit a graph to the server.
    pub async fn submit(
        &self,
        challenge_id: &str,
        graph: RgxfJson,
    ) -> Result<SubmitResponse, SearchError> {
        let url = format!("{}/api/submit", self.base_url);
        let body = SubmitRequest {
            challenge_id: challenge_id.to_string(),
            graph,
        };

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SearchError::ServerError(format!("{status}: {body}")));
        }

        let submit_resp: SubmitResponse = resp.json().await?;
        Ok(submit_resp)
    }
}
