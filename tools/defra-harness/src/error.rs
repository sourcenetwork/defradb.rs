//! The harness's error: a message, the step it describes, and its cause.

use std::fmt;

/// The harness's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// What a harness step failed with.
#[derive(thiserror::Error)]
pub enum Error {
    /// A failure the harness states itself.
    #[error("{0}")]
    Message(String),
    /// A step that failed, and what it failed with.
    #[error("{context}: {source}")]
    Context {
        /// The step.
        context: String,
        /// Its failure.
        source: Box<Error>,
    },
    /// A node exited because a port allocated to it was already bound.
    #[error("node failed to bind an allocated port (address already in use): {0}")]
    PortConflict(Box<Error>),
    /// A file, directory or process operation.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An HTTP request.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// A JSON document.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Bytes that are not UTF-8.
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
}

impl Error {
    /// A failure stated as text.
    pub fn msg<M>(message: M) -> Self
    where
        M: fmt::Display,
    {
        Self::Message(message.to_string())
    }

    /// Whether a node failed to bind a port the harness had allocated to it,
    /// at any depth of the failure. Such a start can be retried.
    pub fn is_port_conflict(&self) -> bool {
        match self {
            Self::PortConflict(_) => true,
            Self::Context { source, .. } => source.is_port_conflict(),
            _ => false,
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Names the step a failure belongs to.
pub trait WrapErr<T> {
    /// The failure, under `context`.
    fn wrap_err<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display;

    /// The failure, under the context `context` builds.
    fn wrap_err_with<C, F>(self, context: F) -> Result<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C;
}

impl<T, E> WrapErr<T> for std::result::Result<T, E>
where
    E: Into<Error>,
{
    fn wrap_err<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display,
    {
        self.wrap_err_with(|| context)
    }

    fn wrap_err_with<C, F>(self, context: F) -> Result<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C,
    {
        self.map_err(|source| Error::Context {
            context: context().to_string(),
            source: Box::new(source.into()),
        })
    }
}

impl<T> WrapErr<T> for Option<T> {
    fn wrap_err<C>(self, context: C) -> Result<T>
    where
        C: fmt::Display,
    {
        self.ok_or_else(|| Error::msg(context))
    }

    fn wrap_err_with<C, F>(self, context: F) -> Result<T>
    where
        C: fmt::Display,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| Error::msg(context()))
    }
}

/// An [`Error`] from a format string.
macro_rules! err {
    ($($arg:tt)+) => {
        $crate::error::Error::msg(format!($($arg)+))
    };
}

/// Returns an [`Error`] from a format string.
macro_rules! bail {
    ($($arg:tt)+) => {
        return Err($crate::error::err!($($arg)+))
    };
}

/// Returns an [`Error`] from a format string unless the condition holds.
macro_rules! ensure {
    ($cond:expr, $($arg:tt)+) => {
        if !$cond {
            $crate::error::bail!($($arg)+);
        }
    };
}

pub(crate) use {bail, ensure, err};
