//! The ID of an explicit transaction, as every edge names it.

use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// An explicit transaction's ID: a per-start token in the high bits and the
/// owner route in the low bits of the numeric ID the HTTP routes carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxnId {
    /// The process start that opened the transaction.
    pub start: u64,
    /// The route that names the owner worker and its sequence.
    pub route: u64,
}

/// The start token's bits in a new ID.
const START_MASK: u64 = u64::MAX << 32;
/// The owner route's bits in a new ID.
const ROUTE_MASK: u64 = u32::MAX as u64;

impl TxnId {
    /// The numeric ID the wire carries: the start token and route packed
    /// together. A legacy route keeps its whole numeric value.
    pub fn encoded(self) -> u64 {
        self.start | self.route
    }

    /// The canonical transaction ID from its numeric wire value.
    pub fn from_wire(encoded: u64) -> TxnId {
        let start = encoded & START_MASK;
        if start == 0 {
            return TxnId {
                start: 0,
                route: encoded,
            };
        }
        TxnId {
            start,
            route: encoded & ROUTE_MASK,
        }
    }
}

/// The text was not a decimal `u64`, the packed start token and route.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("invalid transaction id")]
pub struct TxnIdParseError;

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.encoded())
    }
}

impl FromStr for TxnId {
    type Err = TxnIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse()
            .map(TxnId::from_wire)
            .map_err(|_| TxnIdParseError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        assert_eq!(
            ((9u64 << 32) | 42).to_string().parse::<TxnId>(),
            Ok(TxnId {
                start: 9u64 << 32,
                route: 42
            })
        );
        assert_eq!(
            TxnId {
                start: 9u64 << 32,
                route: 42
            }
            .to_string(),
            ((9u64 << 32) | 42).to_string()
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
