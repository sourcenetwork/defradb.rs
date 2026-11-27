/// Why a backup export, a backup import or a store dump failed. HTTP, the
/// CLI, FFI and the embedded node all report these texts unchanged.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// The node refused the caller, or its storage failed.
    #[error(transparent)]
    Db(#[from] crate::Error),
    /// The export could not read or write its documents.
    #[error("{0}")]
    Export(String),
    /// The import could not read its file or save a document.
    #[error("{0}")]
    Import(String),
    /// The store could not be read.
    #[error("{0}")]
    Dump(String),
}
