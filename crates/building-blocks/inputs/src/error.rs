//! Why a service could not answer a key.

use std::fmt;
use std::time::Duration;

/// Why a key has no answer. `E` is the source's own error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<E> {
    /// The remote side answered with an error, after the retries it allowed.
    Source(E),
    /// No answer came within the deadline.
    Deadline(Duration),
    /// The runtime stopped the round trip before it ended.
    Closed,
    /// The round trip panicked; only its own keys failed.
    Panicked,
    /// The source answered a different number of keys than it was asked.
    Answers {
        /// Keys asked.
        expected: usize,
        /// Answers returned.
        got: usize,
    },
}

impl<E> Error<E> {
    /// The source's own error, when that is why.
    pub fn source_error(&self) -> Option<&E> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl<E> fmt::Display for Error<E>
where
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(f),
            Self::Deadline(limit) => write!(f, "no answer within {limit:?}"),
            Self::Closed => f.write_str("the network runtime stopped before the answer came"),
            Self::Panicked => f.write_str("the request panicked"),
            Self::Answers { expected, got } => {
                write!(f, "asked for {expected} answers, got {got}")
            }
        }
    }
}

impl<E> std::error::Error for Error<E> where E: fmt::Display + fmt::Debug {}
