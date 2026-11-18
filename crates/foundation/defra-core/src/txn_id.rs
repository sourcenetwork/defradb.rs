//! The ID of an explicit transaction, as every edge names it.

use std::fmt;
use std::str::FromStr;

/// An explicit transaction's ID: Go's `uint64`, the number the HTTP routes
/// carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxnId(pub u64);

impl fmt::Display for TxnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for TxnId {
    type Err = std::num::ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.parse().map(TxnId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_text() {
        assert_eq!("42".parse::<TxnId>(), Ok(TxnId(42)));
        assert_eq!(TxnId(42).to_string(), "42");
        assert!("x".parse::<TxnId>().is_err());
    }
}
