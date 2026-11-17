//! What a node still owes an external provider, as its callers are told.
//!
//! An entry is a registration a create owes or a relationship change a
//! transaction staged. It says where the entry stands and why, and never the
//! credential recorded with it.

use identity::Did;

use super::record::{OwedAuthority, OwedOp, OwedState, OwedSubject};

/// Names one thing owed to the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedRef {
    /// The registration of one object.
    Registration {
        policy_id: String,
        resource: String,
        object: String,
    },
    /// The relationship change at one place of the node's queue.
    Change { seq: u64 },
}

impl std::fmt::Display for OwedRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registration {
                policy_id,
                resource,
                object,
            } => write!(
                f,
                "registration of {resource} {object} under policy {policy_id}"
            ),
            Self::Change { seq } => write!(f, "change {seq}"),
        }
    }
}

/// What an entry asks of the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedWhat {
    /// Register the object with the entry's actor as its owner.
    Registration,
    /// Add or delete one relationship on the object.
    Change {
        seq: u64,
        op: OwedOp,
        relation: String,
        subject: OwedSubject,
    },
}

/// What an entry waits behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwedHold {
    /// The registration its object still owes.
    Registration,
    /// An earlier change of its relationship. `seq` is that change's place
    /// in the queue when the same actor staged it, and `None` when another
    /// actor did: an entry tells its reader nothing of another's.
    Change { seq: Option<u64> },
}

/// The kind of authority recorded with an entry. The credential itself is
/// never reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwedAuthorityKind {
    /// The node signs with a key it holds for the actor.
    NodeKey,
    /// A delegation the actor presented, kept with the entry.
    Delegation,
}

impl From<&OwedAuthority> for OwedAuthorityKind {
    fn from(authority: &OwedAuthority) -> Self {
        match authority {
            OwedAuthority::NodeKey => Self::NodeKey,
            OwedAuthority::Delegation { .. } => Self::Delegation,
        }
    }
}

/// One thing a node owes the provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwedEntry {
    pub what: OwedWhat,
    /// The DID the provider is asked as: a registration's creator, a
    /// change's acting DID.
    pub actor: Did,
    pub policy_id: String,
    pub resource: String,
    pub object: String,
    /// The explicit transaction that recorded the entry, where there was
    /// one.
    pub txn: Option<u64>,
    /// Where it stands: staged, waiting for authority, or refused.
    pub state: OwedState,
    /// Why its last submission failed, in the provider's words or the
    /// node's.
    pub reason: Option<String>,
    /// Submissions the provider answered with an error, one after the other.
    pub attempts: u32,
    /// What it waits behind: its object's owed registration, or an earlier
    /// change of its tuple.
    pub held_by: Option<OwedHold>,
    /// What is recorded to send it with; `None` when nothing is.
    pub authority: Option<OwedAuthorityKind>,
    /// Whether a submission by the node could still move it as things
    /// stand. A change is sendable when it is staged and what it waits
    /// behind is. A registration is when it is staged, or waits for
    /// authority that the node now has: its own record's again, or the
    /// delegation of a change its creator owes on the same object. An entry
    /// that is not sendable stays as it is until its actor hands in a fresh
    /// credential for it or for what it waits behind, or abandons it.
    pub sendable: bool,
    /// The provider does not hold this change and never will through this
    /// record, which is gone: the provider refused a change nobody had
    /// checked, cannot take one of its kind, a later change of its
    /// relationship passed it, or its actor abandoned it. `reason` says
    /// which. Such an entry is a note: it is shown until its actor was told
    /// by a wait or by the answer to a fresh credential.
    pub discarded: bool,
}

impl OwedEntry {
    /// The reference that names this entry.
    pub fn reference(&self) -> OwedRef {
        match &self.what {
            OwedWhat::Registration => OwedRef::Registration {
                policy_id: self.policy_id.clone(),
                resource: self.resource.clone(),
                object: self.object.clone(),
            },
            OwedWhat::Change { seq, .. } => OwedRef::Change { seq: *seq },
        }
    }
}

/// Which entries a report holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwedFilter<'a> {
    /// What one actor owes: the registrations of documents it created and
    /// the changes it staged.
    Actor(&'a Did),
    /// What one explicit transaction recorded.
    Transaction(u64),
    /// One entry.
    Entry(&'a OwedRef),
}

/// What became of an entry a fresh credential was handed in for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedOutcome {
    /// The provider holds it and its record is gone.
    Settled,
    /// It is still owed, as the entry says.
    Owed(Box<OwedEntry>),
    /// Its record was deleted without the provider holding it, for the
    /// reason the entry gives.
    Discarded(Box<OwedEntry>),
}

/// Where what one transaction owes the provider has come to rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedWait {
    /// The provider holds everything the transaction owed it.
    Settled,
    /// Nothing more will be sent as things stand: every entry left is
    /// parked, waits behind one that is, or was discarded
    /// ([`OwedEntry::discarded`]). A discarded entry is told once: the wait
    /// that returns it deletes its note.
    Parked(Vec<OwedEntry>),
    /// Entries the node can still send are left: the wait ended first, or
    /// the provider could not be reached.
    Pending {
        entries: Vec<OwedEntry>,
        provider_unavailable: bool,
    },
}

impl OwedWait {
    /// The entries still owed, in whatever state.
    pub fn entries(&self) -> &[OwedEntry] {
        match self {
            Self::Settled => &[],
            Self::Parked(entries) | Self::Pending { entries, .. } => entries,
        }
    }

    pub(crate) fn of(entries: Vec<OwedEntry>, provider_unavailable: bool) -> Self {
        if entries.is_empty() {
            Self::Settled
        } else if entries.iter().any(|entry| entry.sendable) {
            Self::Pending {
                entries,
                provider_unavailable,
            }
        } else {
            Self::Parked(entries)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64, actor: &str, sendable: bool) -> OwedEntry {
        OwedEntry {
            what: OwedWhat::Change {
                seq,
                op: OwedOp::Add,
                relation: "reader".into(),
                subject: OwedSubject::Wildcard,
            },
            actor: Did::new(format!("did:key:{actor}")).unwrap(),
            policy_id: "policy".into(),
            resource: "users".into(),
            object: "bae-1".into(),
            txn: Some(1),
            state: if sendable {
                OwedState::Staged
            } else {
                OwedState::AwaitingAuthority
            },
            reason: None,
            attempts: 0,
            held_by: None,
            authority: Some(OwedAuthorityKind::Delegation),
            sendable,
            discarded: false,
        }
    }

    #[test]
    fn a_wait_says_settled_parked_or_pending() {
        assert_eq!(OwedWait::of(Vec::new(), false), OwedWait::Settled);
        assert!(matches!(
            OwedWait::of(vec![entry(1, "a", false)], false),
            OwedWait::Parked(entries) if entries.len() == 1
        ));
        assert!(matches!(
            OwedWait::of(vec![entry(1, "a", false), entry(2, "a", true)], true),
            OwedWait::Pending { entries, provider_unavailable: true } if entries.len() == 2
        ));
    }

    #[test]
    fn an_entry_names_itself() {
        assert_eq!(entry(7, "a", true).reference(), OwedRef::Change { seq: 7 });
        let registration = OwedEntry {
            what: OwedWhat::Registration,
            ..entry(0, "a", true)
        };
        assert_eq!(
            registration.reference(),
            OwedRef::Registration {
                policy_id: "policy".into(),
                resource: "users".into(),
                object: "bae-1".into(),
            }
        );
        assert_eq!(
            registration.reference().to_string(),
            "registration of users bae-1 under policy policy"
        );

        // A discarded change is at rest, and is not what "settled" means.
        let gone = OwedEntry {
            discarded: true,
            ..entry(3, "a", false)
        };
        assert!(matches!(
            OwedWait::of(vec![gone], false),
            OwedWait::Parked(entries) if entries[0].discarded
        ));
    }
}
