//! The record of one relationship change owed to an external provider.
//!
//! A record is one version byte followed by a JSON object. Version 1 holds
//! the fields of [`OwedChange`] under their own names, with the tuple's
//! fields beside the others, and every enum as an object tagged by `kind` in
//! snake case (`op` is the bare string `add` or `delete`). A reader ignores
//! fields it does not know, so a later version of the same byte may add some.
//!
//! `authority_checked` is such a field. It is written only when it is
//! `false`, and a record without it reads as checked, so a record of a
//! checked change keeps the bytes it always had.

use identity::Did;
use serde::{Deserialize, Serialize};
use zanzibar::types::Subject;

use crate::error::{Error, Result};

/// The most bytes one encoded record may take. A record carries its caller's
/// delegation and the provider's last refusal, both bounded by this.
pub const MAX_OWED_CHANGE_BYTES: usize = 16 * 1024;

const RECORD_VERSION: u8 = 1;

/// The bytes of the provider's words a parked record is always left room
/// for, in its state and again in its last error. Longer words are cut to
/// what the record has left.
pub const RESERVED_REASON_BYTES: usize = 128;

/// What tells this run of the process from every other. A transaction is
/// known by the number of its handle, and those numbers start again when the
/// node does: a record that names its transaction names the run too, so a
/// later run's transaction of the same number is not taken for it.
pub fn run_epoch() -> u64 {
    static EPOCH: regolith::sync::OnceCell<u64> = regolith::sync::OnceCell::new();
    *EPOCH.get_or_init_racy(|| {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos() as u64);
        since_epoch ^ (u64::from(std::process::id()) << 32)
    })
}

/// Whether a record that names `txn` in run `epoch` was made by the
/// transaction `wanted` of this run.
pub(crate) fn of_this_run(txn: Option<u64>, epoch: Option<u64>, wanted: u64) -> bool {
    txn == Some(wanted) && epoch == Some(run_epoch())
}

/// What the change does to its tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwedOp {
    Add,
    Delete,
}

/// The subject of an owed tuple, in a form this module owns: the stored
/// bytes do not follow another crate's type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwedSubject {
    /// One actor.
    Actor { did: Did },
    /// Every actor (`*`).
    Wildcard,
    /// The holders of `relation` on another object.
    EntitySet {
        resource: String,
        object: String,
        relation: String,
    },
    /// Every object of `resource`.
    TypedWildcard { resource: String },
}

impl TryFrom<Subject> for OwedSubject {
    type Error = Error;

    /// The all-actors DID is the wildcard, however its caller spelled it.
    fn try_from(subject: Subject) -> Result<Self> {
        Ok(match subject {
            Subject::Entity(did) if did.is_wildcard() => Self::Wildcard,
            Subject::Entity(did) => Self::Actor { did },
            Subject::Wildcard => Self::Wildcard,
            Subject::EntitySet {
                resource,
                object_id,
                relation,
            } => Self::EntitySet {
                resource,
                object: object_id,
                relation,
            },
            Subject::TypedWildcard { resource } => Self::TypedWildcard { resource },
            other => return Err(Error::UnsupportedSubject(other.to_string())),
        })
    }
}

impl From<OwedSubject> for Subject {
    fn from(subject: OwedSubject) -> Self {
        match subject {
            OwedSubject::Actor { did } => Self::Entity(did),
            OwedSubject::Wildcard => Self::Wildcard,
            OwedSubject::EntitySet {
                resource,
                object,
                relation,
            } => Self::EntitySet {
                resource,
                object_id: object,
                relation,
            },
            OwedSubject::TypedWildcard { resource } => Self::TypedWildcard { resource },
        }
    }
}

/// The relationship a change adds or deletes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwedTuple {
    pub policy_id: String,
    pub resource: String,
    pub object: String,
    pub relation: String,
    pub subject: OwedSubject,
}

/// What lets the node submit the change as its acting DID.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwedAuthority {
    /// The node holds the acting DID's key and signs at submission.
    NodeKey,
    /// The caller's delegation, as it presented it.
    Delegation { token: String },
}

impl std::fmt::Debug for OwedAuthority {
    /// A delegation is a credential: its token is never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeKey => f.write_str("NodeKey"),
            Self::Delegation { .. } => f.write_str("Delegation { .. }"),
        }
    }
}

/// Where an unsettled change stands. A settled change has no record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwedState {
    /// Committed with its transaction and not yet submitted.
    Staged,
    /// The provider refused it for good.
    Refused { reason: String },
    /// Its authority no longer lets the node submit it.
    AwaitingAuthority,
}

/// A change as its caller stages it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewOwedChange {
    /// The transaction staging it, where its caller knows one.
    pub txn: Option<u64>,
    pub tuple: OwedTuple,
    pub op: OwedOp,
    /// The DID the change is made as.
    pub actor: Did,
    pub authority: OwedAuthority,
    /// Whether the acting DID's authority was checked when the change was
    /// staged.
    pub authority_checked: bool,
}

fn checked() -> bool {
    true
}

fn is_checked(checked: &bool) -> bool {
    *checked
}

/// One relationship change owed to the provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwedChange {
    /// Its place in the queue.
    pub seq: u64,
    pub txn: Option<u64>,
    #[serde(flatten)]
    pub tuple: OwedTuple,
    pub op: OwedOp,
    pub actor: Did,
    pub authority: OwedAuthority,
    pub state: OwedState,
    /// Submissions the provider answered with the error in `last_error`,
    /// one after the other.
    pub attempts: u32,
    pub last_error: Option<String>,
    /// Whether the acting DID's authority was checked when the change was
    /// staged. A change staged unchecked is removed, not parked, when the
    /// provider refuses it.
    #[serde(default = "checked", skip_serializing_if = "is_checked")]
    pub authority_checked: bool,
    /// The run of the process `txn` is a transaction of ([`run_epoch`]);
    /// `None` where no transaction is named, and in a record written before
    /// runs were told apart, whose transaction no later run can name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
}

impl OwedChange {
    pub(crate) fn staged(seq: u64, new: NewOwedChange) -> Self {
        Self {
            seq,
            txn: new.txn,
            tuple: new.tuple,
            op: new.op,
            actor: new.actor,
            authority: new.authority,
            state: OwedState::Staged,
            attempts: 0,
            last_error: None,
            authority_checked: new.authority_checked,
            epoch: new.txn.map(|_| run_epoch()),
        }
    }

    /// What is kept of this change once its record is deleted without the
    /// provider holding it: the change, why it went, and no credential.
    pub(crate) fn discarded(mut self, reason: String) -> Self {
        self.authority = OwedAuthority::NodeKey;
        self.state = OwedState::Refused {
            reason: reason.clone(),
        };
        self.last_error = Some(reason);
        self
    }

    /// The record's bytes as it is first written, refused unless the record
    /// still fits [`MAX_OWED_CHANGE_BYTES`] in the largest state it can be
    /// parked in later: refused or awaiting authority, with the highest
    /// attempt count and [`RESERVED_REASON_BYTES`] of the provider's words
    /// twice. A change that could be staged and never parked would be
    /// submitted again for ever, with its tuple behind it.
    pub(crate) fn encode_admitted(&self) -> Result<Vec<u8>> {
        let words = "x".repeat(RESERVED_REASON_BYTES);
        for state in [
            OwedState::Refused {
                reason: words.clone(),
            },
            OwedState::AwaitingAuthority,
        ] {
            Self {
                state,
                attempts: u32::MAX,
                last_error: Some(words.clone()),
                ..self.clone()
            }
            .encode()?;
        }
        self.encode()
    }

    /// Halve the provider's words kept with the change, in its state and in
    /// its last error. `false` when nothing is left to cut.
    pub(crate) fn shorten_reason(&mut self) -> bool {
        fn halve(text: &mut String) -> bool {
            let mut keep = text.len() / 2;
            while !text.is_char_boundary(keep) {
                keep -= 1;
            }
            let cut = keep < text.len();
            text.truncate(keep);
            cut
        }
        let mut cut = false;
        if let OwedState::Refused { reason } = &mut self.state {
            cut |= halve(reason);
        }
        if let Some(error) = &mut self.last_error {
            cut |= halve(error);
        }
        cut
    }

    /// The record's bytes, refused over [`MAX_OWED_CHANGE_BYTES`].
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = vec![RECORD_VERSION];
        serde_json::to_writer(&mut bytes, self)?;
        if bytes.len() > MAX_OWED_CHANGE_BYTES {
            return Err(Error::OwedChangeTooLarge {
                size: bytes.len(),
                limit: MAX_OWED_CHANGE_BYTES,
            });
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        match bytes.split_first() {
            Some((&RECORD_VERSION, body)) => Ok(serde_json::from_slice(body)?),
            Some((version, _)) => Err(Error::Serialization(format!(
                "owed ACP change record has unknown version {version}"
            ))),
            None => Err(Error::Serialization(
                "owed ACP change record is empty".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did(name: &str) -> Did {
        Did::new(format!("did:key:{name}")).unwrap()
    }

    fn change(subject: OwedSubject, authority: OwedAuthority, state: OwedState) -> OwedChange {
        OwedChange {
            seq: 7,
            txn: Some(42),
            tuple: OwedTuple {
                policy_id: "policy".into(),
                resource: "users".into(),
                object: "bae-1".into(),
                relation: "reader".into(),
                subject,
            },
            op: OwedOp::Delete,
            actor: did("owner"),
            authority,
            state,
            attempts: 3,
            last_error: Some("provider unreachable".into()),
            authority_checked: true,
            epoch: None,
        }
    }

    #[test]
    fn every_variant_round_trips() {
        let subjects = [
            OwedSubject::Actor { did: did("reader") },
            OwedSubject::Wildcard,
            OwedSubject::EntitySet {
                resource: "groups".into(),
                object: "team/#1".into(),
                relation: "member".into(),
            },
            OwedSubject::TypedWildcard {
                resource: "groups".into(),
            },
        ];
        let authorities = [
            OwedAuthority::NodeKey,
            OwedAuthority::Delegation {
                token: "header.claims.signature".into(),
            },
        ];
        let states = [
            OwedState::Staged,
            OwedState::Refused {
                reason: "not a manager".into(),
            },
            OwedState::AwaitingAuthority,
        ];
        for subject in &subjects {
            for authority in &authorities {
                for state in &states {
                    let change = change(subject.clone(), authority.clone(), state.clone());
                    assert_eq!(
                        OwedChange::decode(&change.encode().unwrap()).unwrap(),
                        change
                    );
                }
            }
        }
    }

    #[test]
    fn version_one_keeps_its_bytes() {
        let mut staged = change(
            OwedSubject::Wildcard,
            OwedAuthority::NodeKey,
            OwedState::Staged,
        );
        staged.txn = None;
        staged.attempts = 0;
        staged.last_error = None;
        let bytes = staged.encode().unwrap();
        assert_eq!(bytes[0], 1);
        assert_eq!(
            std::str::from_utf8(&bytes[1..]).unwrap(),
            concat!(
                r#"{"seq":7,"txn":null,"policy_id":"policy","resource":"users","#,
                r#""object":"bae-1","relation":"reader","subject":{"kind":"wildcard"},"#,
                r#""op":"delete","actor":"did:key:owner","authority":{"kind":"node_key"},"#,
                r#""state":{"kind":"staged"},"attempts":0,"last_error":null}"#
            )
        );
    }

    #[test]
    fn an_unchecked_change_says_so_and_a_record_without_the_field_is_checked() {
        let mut unchecked = change(
            OwedSubject::Wildcard,
            OwedAuthority::NodeKey,
            OwedState::Staged,
        );
        let checked_bytes = unchecked.encode().unwrap();
        assert!(!String::from_utf8_lossy(&checked_bytes).contains("authority_checked"));
        assert!(
            OwedChange::decode(&checked_bytes)
                .unwrap()
                .authority_checked
        );

        unchecked.authority_checked = false;
        let bytes = unchecked.encode().unwrap();
        assert!(String::from_utf8_lossy(&bytes).ends_with(r#","authority_checked":false}"#));
        assert_eq!(OwedChange::decode(&bytes).unwrap(), unchecked);
    }

    #[test]
    fn a_record_over_the_cap_is_refused_and_one_at_it_is_kept() {
        let mut large = change(
            OwedSubject::Wildcard,
            OwedAuthority::Delegation {
                token: String::new(),
            },
            OwedState::Staged,
        );
        let empty = large.encode().unwrap().len();
        large.authority = OwedAuthority::Delegation {
            token: "t".repeat(MAX_OWED_CHANGE_BYTES - empty),
        };
        assert_eq!(large.encode().unwrap().len(), MAX_OWED_CHANGE_BYTES);

        large.authority = OwedAuthority::Delegation {
            token: "t".repeat(MAX_OWED_CHANGE_BYTES - empty + 1),
        };
        assert!(matches!(
            large.encode(),
            Err(Error::OwedChangeTooLarge { size, limit })
                if size == MAX_OWED_CHANGE_BYTES + 1 && limit == MAX_OWED_CHANGE_BYTES
        ));
    }

    #[test]
    fn an_unknown_version_and_an_empty_record_are_refused() {
        let mut bytes = change(
            OwedSubject::Wildcard,
            OwedAuthority::NodeKey,
            OwedState::Staged,
        )
        .encode()
        .unwrap();
        bytes[0] = 2;
        assert!(matches!(
            OwedChange::decode(&bytes),
            Err(Error::Serialization(_))
        ));
        assert!(matches!(
            OwedChange::decode(&[]),
            Err(Error::Serialization(_))
        ));
    }

    #[test]
    fn the_all_actors_did_is_the_wildcard() {
        assert_eq!(
            OwedSubject::try_from(Subject::Entity(Did::wildcard())).unwrap(),
            OwedSubject::Wildcard
        );
        assert_eq!(
            Subject::from(OwedSubject::try_from(Subject::Wildcard).unwrap()),
            Subject::Wildcard
        );
    }

    #[test]
    fn a_delegation_is_not_printed() {
        let authority = OwedAuthority::Delegation {
            token: "secret".into(),
        };
        assert!(!format!("{authority:?}").contains("secret"));
    }
}
