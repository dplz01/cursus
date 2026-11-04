use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("libsimplex not found (looked in {searched}). Run ./scripts/fetch-libs.sh")]
    LibraryNotFound { searched: String },

    #[error("failed to load libsimplex: {0}")]
    LibraryLoad(String),

    #[error("missing symbol {0} in libsimplex")]
    MissingSymbol(String),

    #[error("chat controller was not created")]
    NoController,

    #[error("database migration failed: {0}")]
    Migration(String),

    #[error("chat command error: {0}")]
    Command(String),

    #[error("invalid JSON from libsimplex: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
