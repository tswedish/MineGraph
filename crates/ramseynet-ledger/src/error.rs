use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("challenge not found: {0}")]
    ChallengeNotFound(String),

    #[error("challenge already exists: {0}")]
    ChallengeAlreadyExists(String),

    #[error("graph already submitted: {0}")]
    GraphAlreadySubmitted(String),

    #[error("graph not found: {0}")]
    GraphNotFound(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
