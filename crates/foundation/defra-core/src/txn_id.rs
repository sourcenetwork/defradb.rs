//! The ID of an explicit transaction, as every edge names it.

use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// An explicit transaction's ID: an opaque per-start token and the owner route
/// the HTTP routes carry as `<start>-<route>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxnId {
    /// The process start that opened the transaction.
    pub start: u64,
    /// The route that names the owner worker and its sequence.
    pub route: u64,
}

/// The text was not `<start>-<route>`.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("transaction ID must be `<start>-<route>` or a legacy numeric route")]
pub struct TxnIdParseError;

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.route)
    }
}

impl FromStr for TxnId {
    type Err = TxnIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let Some((start, route)) = text.split_once('-') else {
            let route = text.parse().map_err(|_| TxnIdParseError)?;
            return Ok(TxnId { start: 0, route });
        };
        let (start, route) = (
            start.parse().ok().ok_or(TxnIdParseError)?,
            route.parse().ok().ok_or(TxnIdParseError)?,
        );
        Ok(TxnId { start, route })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        assert_eq!(
            "9-42".parse::<TxnId>(),
            Ok(TxnId {
                start: 9,
                route: 42
            })
        );
        assert_eq!(
            TxnId {
                start: 9,
                route: 42
            }
            .to_string(),
            "9-42"
        );
        assert_eq!(
            "42".parse::<TxnId>(),
            Ok(TxnId {
                start: 0,
                route: 42
            })
        );
        assert!("x".parse::<TxnId>().is_err());
    }
}
