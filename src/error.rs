use bindb::StoreError;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CodeintelError {
    #[error("git command failed: {0}")]
    Git(String),

    #[error("not a git repository: {0}")]
    NotGitRepo(PathBuf),

    #[error("parse error in {path}: {message}")]
    Parse { path: String, message: String },

    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("bindb payload for {0} is missing or not JSON")]
    BadPayload(String),

    #[error("{0}")]
    Other(String),

    #[error("stale_version")]
    StaleVersion {
        repo: String,
        id: String,
        current_version: u64,
    },
}

pub type Result<T> = std::result::Result<T, CodeintelError>;
