//! Registrations a create owes an external ACP provider.
//!
//! An external provider (Vera) cannot join the create's transaction, so the
//! create writes a record of the registration it owes into that transaction.
//! The record commits with the document: until the provider confirms the
//! registration and the record is cleared, the document is readable only by
//! its creator, and a crash or a failed provider call leaves the record for a
//! redrive to complete. A record exists only for a document its transaction
//! created, so an upsert that found an existing document owes nothing.
//!
//! The record says what lets the node send the registration as the creator
//! ([`PendingRegistration::authority`]), so a later submission needs nothing
//! from the request that made the create, and what its last submission came
//! to ([`PendingRegistration::state`], [`PendingRegistration::last_error`]).
//! A record written before these fields existed reads as owed with nothing
//! recorded: the fields are written only when they say something, so such a
//! record keeps its bytes.

use datastore::{NamespaceView, WeakNamespaceView};
use identity::Did;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::outbox::{OwedAuthority, OwedState};

/// The most bytes of a failed submission's reason a record keeps.
const MAX_REASON_BYTES: usize = 512;

fn staged() -> OwedState {
    OwedState::Staged
}

fn is_staged(state: &OwedState) -> bool {
    *state == OwedState::Staged
}

fn is_plain(request_bound: &bool) -> bool {
    !*request_bound
}

fn is_zero(attempts: &u32) -> bool {
    *attempts == 0
}

/// Start of every owed-registration key in the ACP namespace.
pub(crate) const PENDING_PREFIX: &[u8] = b"/acp-pending/";

/// One owed registration: `creator` becomes the owner of `doc_id`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingRegistration {
    pub creator: Did,
    pub policy_id: String,
    pub resource_name: String,
    pub doc_id: String,
    /// What lets the node send the registration as `creator`: its own key
    /// for that DID, or the delegation the create carried, as presented.
    /// `None` when the create carried nothing the provider takes: the
    /// registration then waits for a credential
    /// ([`crate::DocumentACP::reauthorize_owed`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<OwedAuthority>,
    /// Whether the recorded delegation was presented as bound to this one
    /// registration. It is presented the same way when the registration is
    /// sent: a provider takes neither kind in the other's place.
    #[serde(default, skip_serializing_if = "is_plain")]
    pub request_bound: bool,
    /// The explicit transaction that made the create, where there was one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub txn: Option<u64>,
    /// The run of the process `txn` is a transaction of
    /// ([`crate::outbox::run_epoch`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
    /// Where the registration stands. It is submitted in every state; the
    /// state says what the last submission came to.
    #[serde(default = "staged", skip_serializing_if = "is_staged")]
    pub state: OwedState,
    /// Submissions the provider answered with an error, one after the other.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub attempts: u32,
    /// Why the last submission failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// The creator of a replicated document this node registers as itself
    /// (see [`crate::replica`]): `creator` is then the node's own actor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_of: Option<Did>,
}

impl PendingRegistration {
    pub fn new(creator: Did, policy_id: &str, resource_name: &str, doc_id: &str) -> Self {
        Self {
            creator,
            policy_id: policy_id.to_string(),
            resource_name: resource_name.to_string(),
            doc_id: doc_id.to_string(),
            authority: None,
            request_bound: false,
            txn: None,
            epoch: None,
            state: OwedState::Staged,
            attempts: 0,
            last_error: None,
            replica_of: None,
        }
    }

    /// This registration made by the receiving node for a replicated
    /// document whose genesis `creator` signed.
    #[must_use]
    pub fn for_replica_of(mut self, creator: Did) -> Self {
        self.replica_of = Some(creator);
        self
    }

    /// This registration sent with `authority`, a delegation presented as
    /// bound to it when `request_bound`, or parked as waiting for authority,
    /// with the reason, when the create carried none.
    #[must_use]
    pub fn with_authority(mut self, authority: Result<OwedAuthority>, request_bound: bool) -> Self {
        match authority {
            Ok(authority) => {
                self.request_bound =
                    request_bound && matches!(authority, OwedAuthority::Delegation { .. });
                self.authority = Some(authority);
            }
            Err(error) => self.park(OwedState::AwaitingAuthority, error.to_string()),
        }
        self
    }

    /// This registration made by the explicit transaction `txn`.
    #[must_use]
    pub fn in_transaction(mut self, txn: Option<u64>) -> Self {
        self.txn = txn;
        self.epoch = txn.map(|_| crate::outbox::run_epoch());
        self
    }

    /// Record what a submission came to and why, the reason cut to what a
    /// record keeps.
    pub(crate) fn park(&mut self, state: OwedState, reason: String) {
        let cut = |mut text: String| {
            let mut keep = text.len().min(MAX_REASON_BYTES);
            while !text.is_char_boundary(keep) {
                keep -= 1;
            }
            text.truncate(keep);
            text
        };
        self.state = match state {
            OwedState::Refused { reason } => OwedState::Refused {
                reason: cut(reason),
            },
            other => other,
        };
        self.last_error = Some(cut(reason));
    }

    /// Whether `other` records the same create: everything but what its
    /// submissions came to.
    pub(crate) fn same_create(&self, other: &Self) -> bool {
        self.creator == other.creator
            && self.policy_id == other.policy_id
            && self.resource_name == other.resource_name
            && self.doc_id == other.doc_id
            && self.txn == other.txn
            && self.epoch == other.epoch
    }

    /// The key this registration is recorded under.
    pub fn key(&self) -> Vec<u8> {
        pending_key(&self.policy_id, &self.resource_name, &self.doc_id)
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// The key of the registration owed for one document. Each part is preceded by
/// its length, so no two (policy, resource, document) triples share a key.
pub fn pending_key(policy_id: &str, resource_name: &str, doc_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        PENDING_PREFIX.len() + 24 + policy_id.len() + resource_name.len() + doc_id.len(),
    );
    key.extend_from_slice(PENDING_PREFIX);
    for part in [policy_id, resource_name, doc_id] {
        key.extend_from_slice(&(part.len() as u64).to_be_bytes());
        key.extend_from_slice(part.as_bytes());
    }
    key
}

/// Owed registrations as one transaction sees them: its own writes over its
/// snapshot. Reads are validated point reads, so a registration settled or
/// recorded by a transaction that commits first aborts a decision made on it.
/// The handle does not keep the transaction in use: once it is committed,
/// rolled back or dropped, nothing is owed through it.
#[derive(Clone)]
pub struct PendingRegistrations {
    view: WeakNamespaceView,
}

impl PendingRegistrations {
    /// `view` is the transaction's ACP namespace.
    pub fn new(view: &NamespaceView) -> Self {
        Self {
            view: view.downgrade(),
        }
    }

    fn active_view(&self) -> Result<NamespaceView> {
        self.view
            .upgrade()
            .ok_or_else(|| Error::Storage("the transaction is no longer active".into()))
    }

    /// Record, in this transaction, the registration its create owes.
    pub async fn record(&self, pending: &PendingRegistration) -> Result<()> {
        self.active_view()?
            .set(&pending.key(), &pending.encode()?)
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }

    /// The registration owed for a document, if any.
    pub async fn get(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<PendingRegistration>> {
        let Some(view) = self.view.upgrade() else {
            return Ok(None);
        };
        view.get(&pending_key(policy_id, resource_name, doc_id))
            .await
            .map_err(|error| Error::Storage(error.to_string()))?
            .map(|bytes| PendingRegistration::decode(&bytes))
            .transpose()
    }

    /// Drop the registration owed for a document this transaction deletes.
    pub async fn clear(&self, policy_id: &str, resource_name: &str, doc_id: &str) -> Result<()> {
        self.active_view()?
            .delete(&pending_key(policy_id, resource_name, doc_id))
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_distinct_triples_differ() {
        let triples = [
            ("ab", "c", "d"),
            ("a", "bc", "d"),
            ("a", "b", "cd"),
            ("", "abc", "d"),
            ("abc", "", "d"),
        ];
        for (i, left) in triples.iter().enumerate() {
            for right in &triples[i + 1..] {
                assert_ne!(
                    pending_key(left.0, left.1, left.2),
                    pending_key(right.0, right.1, right.2),
                    "{left:?} and {right:?} share a key"
                );
            }
        }
    }

    #[test]
    fn a_record_round_trips_and_names_its_key() {
        let pending = PendingRegistration::new(
            Did::new("did:key:creator").unwrap(),
            "policy",
            "users",
            "bae-1",
        );
        assert_eq!(
            PendingRegistration::decode(&pending.encode().unwrap()).unwrap(),
            pending
        );
        assert_eq!(pending.key(), pending_key("policy", "users", "bae-1"));
        assert!(pending.key().starts_with(PENDING_PREFIX));
    }

    #[test]
    fn a_record_without_the_later_fields_keeps_its_bytes_and_reads_as_owed() {
        let old = r#"{"creator":"did:key:creator","policy_id":"policy","resource_name":"users","doc_id":"bae-1"}"#;
        let pending = PendingRegistration::decode(old.as_bytes()).unwrap();
        assert_eq!(pending.authority, None);
        assert_eq!(pending.txn, None);
        assert_eq!(pending.state, OwedState::Staged);
        assert_eq!(pending.attempts, 0);
        assert_eq!(pending.last_error, None);
        assert_eq!(pending.replica_of, None);
        assert_eq!(pending.encode().unwrap(), old.as_bytes());
    }

    #[test]
    fn a_record_keeps_its_authority_and_what_its_submission_came_to() {
        let creator = Did::new("did:key:creator").unwrap();
        let recorded = PendingRegistration::new(creator.clone(), "policy", "users", "bae-1")
            .with_authority(
                Ok(OwedAuthority::Delegation {
                    token: "header.claims.signature".into(),
                }),
                true,
            )
            .in_transaction(Some(7));
        assert_eq!(recorded.state, OwedState::Staged);
        assert!(recorded.request_bound);
        let by_key = PendingRegistration::new(creator.clone(), "policy", "users", "bae-1")
            .with_authority(Ok(OwedAuthority::NodeKey), true);
        assert!(!by_key.request_bound, "a key is bound to no request");
        assert_eq!(
            PendingRegistration::decode(&recorded.encode().unwrap()).unwrap(),
            recorded
        );
        assert!(!format!("{recorded:?}").contains("header.claims.signature"));

        let parked = PendingRegistration::new(creator, "policy", "users", "bae-1").with_authority(
            Err(Error::OwedAuthorityUnavailable {
                actor: "did:key:creator".into(),
                reason: "the request carries no delegation".into(),
            }),
            false,
        );
        assert_eq!(parked.authority, None);
        assert_eq!(parked.state, OwedState::AwaitingAuthority);
        assert!(parked
            .last_error
            .as_deref()
            .is_some_and(|reason| reason.contains("carries no delegation")));
        assert_eq!(
            PendingRegistration::decode(&parked.encode().unwrap()).unwrap(),
            parked
        );
        assert!(parked.same_create(&recorded.clone().in_transaction(None)));
        assert!(!parked.same_create(&recorded));
    }

    #[test]
    fn a_long_reason_is_cut_at_a_character() {
        let mut pending = PendingRegistration::new(
            Did::new("did:key:creator").unwrap(),
            "policy",
            "users",
            "bae-1",
        );
        let words = "é".repeat(MAX_REASON_BYTES);
        pending.park(
            OwedState::Refused {
                reason: words.clone(),
            },
            words,
        );
        let kept = pending.last_error.unwrap();
        assert!(kept.len() <= MAX_REASON_BYTES && !kept.is_empty());
        assert!(matches!(pending.state, OwedState::Refused { reason } if reason == kept));
    }
}
