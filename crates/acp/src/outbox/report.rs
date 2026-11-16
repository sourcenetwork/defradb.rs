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
                resource, object, ..
            } => write!(f, "registration of {resource} {object}"),
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
    pub held_by: Option<OwedRef>,
    /// What is recorded to send it with; `None` when nothing is.
    pub authority: Option<OwedAuthorityKind>,
    /// Whether the node can still send it as things stand: it is staged and
    /// waits behind nothing that is parked. An entry that is not sendable
    /// stays as it is until its actor hands in a fresh credential for it or
    /// for what it waits behind, or abandons it.
    pub sendable: bool,
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
}

/// Where what one transaction owes the provider has come to rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwedWait {
    /// The provider holds everything the transaction owed it.
    Settled,
    /// Nothing more will be sent as things stand: every entry left is
    /// parked, or waits behind one that is.
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

    /// What is left of this once only `keep` entries may be seen.
    #[must_use]
    pub fn only(self, keep: impl Fn(&OwedEntry) -> bool) -> Self {
        let (pending, unavailable, entries) = match self {
            Self::Settled => return Self::Settled,
            Self::Parked(entries) => (false, false, entries),
            Self::Pending {
                entries,
                provider_unavailable,
            } => (true, provider_unavailable, entries),
        };
        let entries: Vec<_> = entries.into_iter().filter(|entry| keep(entry)).collect();
        if entries.is_empty() {
            Self::Settled
        } else if pending && entries.iter().any(|entry| entry.sendable) {
            Self::Pending {
                entries,
                provider_unavailable: unavailable,
            }
        } else {
            Self::Parked(entries)
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
    fn a_wait_seen_by_one_actor_holds_only_its_entries() {
        let wait = OwedWait::of(vec![entry(1, "a", false), entry(2, "b", true)], false);
        let own =
            |name: &'static str| move |entry: &OwedEntry| entry.actor.as_str().ends_with(name);
        assert!(matches!(
            wait.clone().only(own("a")),
            OwedWait::Parked(entries) if entries.len() == 1
        ));
        assert!(matches!(
            wait.clone().only(own("b")),
            OwedWait::Pending { entries, .. } if entries.len() == 1
        ));
        assert_eq!(wait.only(own("c")), OwedWait::Settled);
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
    }
}
