use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("graph already submitted: {0}")]
    GraphAlreadySubmitted(String),

    #[error("graph not found: {0}")]
    GraphNotFound(String),

    #[error("below threshold for ({0},{1},n={2})")]
    BelowThreshold(u32, u32, u32),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
