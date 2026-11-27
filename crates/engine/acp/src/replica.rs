//! Replicated documents a receiving node registers with a remote provider.
//!
//! A document created on another deployment arrives at a node whose provider
//! has never heard of it, and its creator cannot write there, so nobody would
//! register it. For a collection its embedder lists, the receiving node
//! registers it as itself and owes, in the same unit, the grants the embedder
//! asks for the creator. The unit is written in the transaction that merges
//! the document's genesis: an owed registration naming the creator it stands
//! for ([`crate::PendingRegistration::replica_of`]), the grants as owed
//! relationship changes, and a marker that says which grants belong to the
//! unit. While any of it is owed, the merge gate puts the creator's changes
//! back to be tried again rather than refusing them.
//!
//! A receiver that finds the document already registered for another owner
//! (another node of the same deployment registered first, or registered it
//! before this node saw the genesis) keeps an adopted marker instead: the
//! owner owes the creator's grants, and the creator's changes wait until the
//! provider lets the creator write. The marker goes once it does.
//!
//! The model is `proofs/tla/ReplicaRegistration.tla`.

use datastore::{NamespaceView, WeakNamespaceView};
use identity::Did;
use serde::{Deserialize, Serialize};
use zanzibar::types::Subject;

use crate::error::{Error, Result};
use crate::outbox::OwedSubject;

/// Start of every replica marker key in the ACP namespace.
pub(crate) const REPLICA_PREFIX: &[u8] = b"/acp-replica/";

/// One relationship an embedder asks a receiver to grant on a document it
/// registers: `relation` for `subject`, such as `writer` for the creator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicaGrant {
    pub relation: String,
    pub subject: Subject,
}

impl ReplicaGrant {
    pub fn new(relation: impl Into<String>, subject: Subject) -> Self {
        Self {
            relation: relation.into(),
            subject,
        }
    }
}

/// A direct membership relation on a group whose owner is trusted by the receiver.
#[derive(Clone, Debug)]
pub struct ReplicaGroup {
    pub policy: String,
    pub resource: String,
    pub object: String,
    pub relation: String,
    pub owner: Did,
}

/// The unit a receiving node owes for a replicated document it registers.
#[derive(Clone, Debug)]
pub struct ReplicaUnit<'a> {
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub doc_id: &'a str,
    /// The DID that signed the document's genesis.
    pub creator: &'a Did,
    /// The DID the node registers the document as: its provider's own actor.
    pub registrant: &'a Did,
    pub grants: &'a [ReplicaGrant],
}

/// One grant of a unit, as its marker keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MarkedGrant {
    pub relation: String,
    pub subject: OwedSubject,
}

/// What a receiver keeps about a replicated document it registered or
/// adopted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ReplicaMarker {
    /// The creator the grants are for, in the provider's spelling.
    pub creator: Did,
    /// The node registered the document as this DID, in the provider's
    /// spelling. `None` for an adopted marker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registrant: Option<Did>,
    /// The owner, not this node, owes the creator's grants.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub adopted: bool,
    /// The grants of this node's unit; empty for an adopted marker.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grants: Vec<MarkedGrant>,
}

impl ReplicaMarker {
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// What a node still owes, or waits for, on a replicated document, as its
/// merge gate reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplicaOwed {
    /// The DID a registration this node still owes for the document names.
    /// No change but that DID's can be decided while it is owed: the
    /// provider has not seen the registration.
    pub registration: Option<Did>,
    /// The creator a unit or an adopted marker is for.
    pub creator: Option<Did>,
    /// A grant of this node's unit is still owed.
    pub creator_grants: bool,
    /// The owner owes the creator's grants, and this node waits for them.
    pub adopted: bool,
}

/// The key of the marker kept for one document. Each part is preceded by its
/// length, as an owed registration's key is.
pub(crate) fn replica_key(policy_id: &str, resource_name: &str, doc_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        REPLICA_PREFIX.len() + 24 + policy_id.len() + resource_name.len() + doc_id.len(),
    );
    key.extend_from_slice(REPLICA_PREFIX);
    for part in [policy_id, resource_name, doc_id] {
        key.extend_from_slice(&(part.len() as u64).to_be_bytes());
        key.extend_from_slice(part.as_bytes());
    }
    key
}

/// Replica markers as one transaction sees them: its own writes over its
/// snapshot, read with validated point reads.
pub(crate) struct ReplicaMarkers {
    view: WeakNamespaceView,
}

impl ReplicaMarkers {
    pub(crate) fn new(view: &NamespaceView) -> Self {
        Self {
            view: view.downgrade(),
        }
    }

    fn active_view(&self) -> Result<NamespaceView> {
        self.view
            .upgrade()
            .ok_or_else(|| Error::Storage("the transaction is no longer active".into()))
    }

    pub(crate) async fn get(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<Option<ReplicaMarker>> {
        self.active_view()?
            .get(&replica_key(policy_id, resource_name, doc_id))
            .await
            .map_err(|error| Error::Storage(error.to_string()))?
            .map(|bytes| ReplicaMarker::decode(&bytes))
            .transpose()
    }

    pub(crate) async fn put(
        &self,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        marker: &ReplicaMarker,
    ) -> Result<()> {
        self.active_view()?
            .set(
                &replica_key(policy_id, resource_name, doc_id),
                &marker.encode()?,
            )
            .await
            .map_err(|error| Error::Storage(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_round_trips_and_an_adopted_one_keeps_few_bytes() {
        let creator = Did::new("did:key:creator").unwrap();
        let unit = ReplicaMarker {
            creator: creator.clone(),
            registrant: Some(Did::new("did:key:node").unwrap()),
            adopted: false,
            grants: vec![MarkedGrant {
                relation: "writer".into(),
                subject: OwedSubject::Actor {
                    did: creator.clone(),
                },
            }],
        };
        assert_eq!(
            ReplicaMarker::decode(&unit.encode().unwrap()).unwrap(),
            unit
        );
        let adopted = ReplicaMarker {
            creator,
            registrant: None,
            adopted: true,
            grants: Vec::new(),
        };
        let bytes = adopted.encode().unwrap();
        assert_eq!(
            std::str::from_utf8(&bytes).unwrap(),
            r#"{"creator":"did:key:creator","adopted":true}"#
        );
        assert_eq!(ReplicaMarker::decode(&bytes).unwrap(), adopted);
    }

    #[test]
    fn marker_keys_of_distinct_documents_differ() {
        assert_ne!(replica_key("ab", "c", "d"), replica_key("a", "bc", "d"));
        assert!(replica_key("p", "r", "d").starts_with(REPLICA_PREFIX));
    }
}
