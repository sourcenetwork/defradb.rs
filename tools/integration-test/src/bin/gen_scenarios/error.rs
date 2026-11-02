use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Mantra(String),
    #[error("rustfmt failed on {path}: {stderr}")]
    Rustfmt { path: PathBuf, stderr: String },
}

pub type Result<T> = std::result::Result<T, GenError>;

pub fn io<T>(path: &std::path::Path, result: std::io::Result<T>) -> Result<T> {
    result.map_err(|source| GenError::Io {
        path: path.to_owned(),
        source,
    })
}
