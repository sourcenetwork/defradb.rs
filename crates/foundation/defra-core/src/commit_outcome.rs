//! What a caller may conclude about a write once its commit has answered.

/// A commit's class, from the engine to the wire (`defra.txn.commit_outcome`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommitOutcome {
    /// Durable and visible to every later transaction.
    Committed,
    /// Nothing the transaction wrote is durable or visible: running it again
    /// cannot apply it twice. On the wire, `aborted`.
    NotCommitted,
    /// The node cannot know whether the commit is durable: a resend may
    /// apply it twice, and only a read after the node recovers can tell.
    Indeterminate,
}

impl CommitOutcome {
    /// The wire value: `committed`, `aborted` or `indeterminate`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::NotCommitted => "aborted",
            Self::Indeterminate => "indeterminate",
        }
    }

    /// The wire value a failure carries; `None` for a success.
    pub fn failure_str(self) -> Option<&'static str> {
        match self {
            Self::Committed => None,
            other => Some(other.as_str()),
        }
    }

    /// The class a wire value names; `None` for any other text.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "committed" => Some(Self::Committed),
            "aborted" => Some(Self::NotCommitted),
            "indeterminate" => Some(Self::Indeterminate),
            _ => None,
        }
    }

    /// The CLI's exit status for a failure of this class: 3 for aborted, 4
    /// for indeterminate. 1 stays the status of an error with no class, and
    /// 2 is clap's usage error.
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Committed => 0,
            Self::NotCommitted => 3,
            Self::Indeterminate => 4,
        }
    }
}

/// The response header a server sets on an answer that refused nothing it
/// could vouch for: a 503 for a request the node cut off at shutdown, whose
/// write may have committed. A client never resends a write so marked.
pub const OUTCOME_HEADER: &str = "x-defradb-outcome";

#[cfg(test)]
mod tests {
    use super::CommitOutcome;

    #[test]
    fn every_class_round_trips_through_its_wire_value() {
        for outcome in [
            CommitOutcome::Committed,
            CommitOutcome::NotCommitted,
            CommitOutcome::Indeterminate,
        ] {
            assert_eq!(CommitOutcome::from_wire(outcome.as_str()), Some(outcome));
        }
        assert_eq!(CommitOutcome::from_wire("unknown"), None);
        assert_eq!(
            CommitOutcome::from_wire("transaction_conflict_aborted"),
            None
        );
    }
}
