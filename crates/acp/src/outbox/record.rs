//! The record of one relationship change owed to an external provider.
//!
//! A record is one version byte followed by a JSON object. Version 1 holds
//! the fields of [`OwedChange`] under their own names, with the tuple's
//! fields beside the others, and every enum as an object tagged by `kind` in
//! snake case (`op` is the bare string `add` or `delete`). A reader ignores
//! fields it does not know, so a later version of the same byte may add some.

use identity::Did;
use serde::{Deserialize, Serialize};
use zanzibar::types::Subject;

use crate::error::{Error, Result};

/// The most bytes one encoded record may take. A record carries its caller's
/// delegation and the provider's last refusal, both bounded by this.
pub const MAX_OWED_CHANGE_BYTES: usize = 16 * 1024;

const RECORD_VERSION: u8 = 1;

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
    /// Submissions tried so far.
    pub attempts: u32,
    pub last_error: Option<String>,
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
        }
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
