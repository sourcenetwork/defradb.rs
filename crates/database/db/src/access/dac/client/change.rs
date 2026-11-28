//! What a relationship change names and what it did.

/// One document relationship: `target` holding `relation` on document
/// `doc_id` of collection `collection`.
#[derive(Clone, Copy)]
pub struct DocRelationship<'a> {
    pub collection: &'a str,
    pub doc_id: &'a str,
    pub relation: &'a str,
    pub target: &'a str,
}

/// What a relationship change made inside a transaction did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationshipChange {
    /// Document ACP joined the transaction and the change is among its
    /// writes: it takes effect, for every reader, when the transaction
    /// commits. `existed` says whether the relationship was there before the
    /// change: an add of one that existed changed nothing, and so did a
    /// delete of one that did not.
    Applied { existed: bool },
    /// Document ACP is a remote provider that cannot join a transaction. The
    /// change is recorded in the transaction as owed to the provider, at
    /// place `seq` of the node's queue, and is submitted after the commit.
    /// Whether the relationship exists on the provider is not known here.
    Staged {
        seq: u64,
        /// Whether the acting DID's authority was checked before staging.
        /// `false` for an add staged while the provider could not be
        /// reached: the provider decides at submission, and the change is
        /// removed if it refuses.
        authority_checked: bool,
    },
}

impl RelationshipChange {
    /// What an add answers as a boolean: whether the relationship existed
    /// already. A staged add answers `false`: it is reported as new.
    pub fn existed_already(self) -> bool {
        match self {
            Self::Applied { existed } => existed,
            Self::Staged { .. } => false,
        }
    }

    /// What a delete answers as a boolean: whether a relationship was found
    /// to delete. A staged delete answers `true`: it is reported as found.
    pub fn record_found(self) -> bool {
        match self {
            Self::Applied { existed } => existed,
            Self::Staged { .. } => true,
        }
    }

    /// The boolean of the change's kind.
    pub(super) fn reported(self, delete: bool) -> bool {
        if delete {
            self.record_found()
        } else {
            self.existed_already()
        }
    }
}
