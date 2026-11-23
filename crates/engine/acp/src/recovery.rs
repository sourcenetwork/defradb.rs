//! The node's own committed access state, for recovering a Vera deployment.

use async_trait::async_trait;
use datastore::NamespaceView;
use identity::Did;
use serde::{Deserialize, Serialize};
use storage::corekv::IterOptions;

use crate::error::{Error, Result};
use crate::outbox::OwedSubject;

/// The start of every object's recovery record in the ACP namespace.
pub(crate) const RECOVERY_PREFIX: &[u8] = b"/acp-recovery/object/";
const STATE_KEY: &[u8] = b"/acp-recovery/state";

/// A page may inspect at most this many encoded records.
pub const RECOVERY_PAGE_BYTES: usize = 256 * 1024;

/// What the node committed to its provider for one object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRelationship {
    /// The actor whose authority put it there.
    pub actor: Did,
    pub relation: String,
    pub subject: OwedSubject,
}

/// One change the node has asked its provider to make.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryOperation {
    Register {
        actor: Did,
    },
    Unregister,
    AddRelationship {
        actor: Did,
        relation: String,
        subject: OwedSubject,
    },
    DeleteRelationship {
        actor: Did,
        relation: String,
        subject: OwedSubject,
    },
}

/// One object's access state, as the node last proved it to its provider.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryObject {
    pub policy_id: String,
    pub resource_name: String,
    pub object_id: String,
    pub owner: Option<Did>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<RecoveryRelationship>,
    /// The latest deletes the node confirmed. A delete removes its grant and
    /// leaves this tombstone, so a recovery pass can remove a grant the
    /// deployment still holds; a later add clears it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tombstones: Vec<RecoveryRelationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<RecoveryOperation>,
}

impl RecoveryObject {
    fn new(policy_id: &str, resource_name: &str, object_id: &str) -> Self {
        Self {
            policy_id: policy_id.to_owned(),
            resource_name: resource_name.to_owned(),
            object_id: object_id.to_owned(),
            ..Self::default()
        }
    }

    fn apply(&mut self, operation: &RecoveryOperation) {
        match operation {
            RecoveryOperation::Register { actor } => self.owner = Some(actor.clone()),
            RecoveryOperation::Unregister => {
                self.owner = None;
                self.relationships.clear();
            }
            RecoveryOperation::AddRelationship {
                actor,
                relation,
                subject,
            } => {
                self.relationships
                    .retain(|held| held.relation != *relation || held.subject != *subject);
                self.tombstones
                    .retain(|held| held.relation != *relation || held.subject != *subject);
                self.relationships.push(RecoveryRelationship {
                    actor: actor.clone(),
                    relation: relation.clone(),
                    subject: subject.clone(),
                });
            }
            RecoveryOperation::DeleteRelationship {
                relation,
                subject,
                actor,
                ..
            } => {
                self.relationships
                    .retain(|held| held.relation != *relation || held.subject != *subject);
                self.tombstones
                    .retain(|held| held.relation != *relation || held.subject != *subject);
                self.tombstones.push(RecoveryRelationship {
                    actor: actor.clone(),
                    relation: relation.clone(),
                    subject: subject.clone(),
                });
            }
        }
    }

    /// The state after the operation the node may have already sent.
    pub fn resolved(&self) -> Self {
        let mut object = self.clone();
        if let Some(pending) = object.pending.take() {
            object.apply(&pending);
        }
        object
    }
}

/// Where a recovery pass stopped: the term it checked and the last record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecoveryCursor {
    pub term: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Vec<u8>>,
    #[serde(default)]
    pub complete: bool,
}

/// What one bounded pass found.
#[derive(Debug, Default)]
pub struct RecoveryPage {
    pub effects: Vec<crate::VoidedEffect>,
    pub cursor: RecoveryCursor,
    /// What this page could not prove. It is reported, never dropped.
    pub unrestored: Vec<String>,
}

/// The deployment, as only a recovery pass reads it.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RecoveryVerifier: defra_core::thread_bounds::MaybeSendSync {
    /// The certified term the pass is rebuilding.
    async fn term(&self) -> std::result::Result<u64, String>;

    async fn object_owner(
        &self,
        term: u64,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> std::result::Result<(bool, String), String>;

    async fn actor_relationship(
        &self,
        term: u64,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        relation: &str,
        subject: &OwedSubject,
    ) -> std::result::Result<bool, String>;
}

/// The node's own access state, read in bounded resumable pages.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RecoverySource: defra_core::thread_bounds::MaybeSendSync {
    async fn next(&self, verifier: &dyn RecoveryVerifier) -> Result<Option<RecoveryPage>>;

    async fn delivered(&self, cursor: RecoveryCursor) -> Result<()>;
}

fn object_key(policy_id: &str, resource_name: &str, object_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        RECOVERY_PREFIX.len() + 24 + policy_id.len() + resource_name.len() + object_id.len(),
    );
    key.extend_from_slice(RECOVERY_PREFIX);
    for part in [policy_id, resource_name, object_id] {
        key.extend_from_slice(&(part.len() as u64).to_be_bytes());
        key.extend_from_slice(part.as_bytes());
    }
    key
}

fn state_error(what: &str, source: impl std::fmt::Display) -> Error {
    Error::Storage(format!("Vera recovery {what}: {source}"))
}

/// One page of recovery records and whether that page reached the ledger's end.
pub(crate) struct LedgerPage {
    pub records: Vec<(Vec<u8>, RecoveryObject)>,
    pub end: bool,
}

/// Recovery records through the transaction whose ACP namespace `view` is.
pub(crate) struct RecoveryLedger {
    view: NamespaceView,
}

impl RecoveryLedger {
    pub(crate) fn new(view: NamespaceView) -> Self {
        Self { view }
    }

    async fn object(
        &self,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> Result<RecoveryObject> {
        self.view
            .get(&object_key(policy_id, resource_name, object_id))
            .await
            .map_err(|error| state_error("read", error))?
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()
            .map_err(|error| state_error("decode", error))
            .map(|object| {
                object.unwrap_or_else(|| RecoveryObject::new(policy_id, resource_name, object_id))
            })
    }

    pub(crate) async fn write(&self, object: RecoveryObject) -> Result<()> {
        let key = object_key(&object.policy_id, &object.resource_name, &object.object_id);
        let bytes = serde_json::to_vec(&object).map_err(|error| state_error("encode", error))?;
        self.view
            .set(&key, &bytes)
            .await
            .map_err(|error| state_error("write", error))
    }

    /// Record that `operation` is about to go to the provider. A pass that
    /// finds it pending applies it first: the provider may or may not hold
    /// it, and every operation is idempotent.
    pub(crate) async fn prepare(
        &self,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        operation: RecoveryOperation,
    ) -> Result<()> {
        let mut object = self.object(policy_id, resource_name, object_id).await?;
        if let Some(pending) = object.pending.take() {
            object.apply(&pending);
        }
        object.pending = Some(operation);
        self.write(object).await
    }

    /// Record what the provider did with `operation`: apply it when it was
    /// applied, otherwise clear the pending mark and leave the state alone.
    pub(crate) async fn settle(
        &self,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        operation: RecoveryOperation,
        applied: bool,
    ) -> Result<()> {
        let mut object = self.object(policy_id, resource_name, object_id).await?;
        let same = object.pending.as_ref() == Some(&operation);
        if applied {
            object.apply(&operation);
        }
        if same {
            object.pending = None;
        }
        self.write(object).await
    }

    /// Every record after `after`, until their encoded bytes pass
    /// `max_bytes`, and whether that page reached the ledger's end.
    pub(crate) async fn page(&self, after: Option<&[u8]>, max_bytes: usize) -> Result<LedgerPage> {
        let mut options = IterOptions::new().with_prefix(RECOVERY_PREFIX.to_vec());
        if let Some(after) = after {
            let mut start = RECOVERY_PREFIX.to_vec();
            start.extend_from_slice(after);
            start.push(0);
            options = options.with_start(start);
        }
        let mut iter = self
            .view
            .iterator(options)
            .await
            .map_err(|error| state_error("open recovery page", error))?;
        let mut records = Vec::new();
        let mut bytes = 0;
        let end = loop {
            let Some(entry) = iter
                .next()
                .await
                .map_err(|error| state_error("read recovery page", error))?
            else {
                break true;
            };
            bytes += entry.value.len();
            let object: RecoveryObject = serde_json::from_slice(&entry.value)
                .map_err(|error| state_error("decode recovery page", error))?;
            let suffix = entry
                .key
                .strip_prefix(RECOVERY_PREFIX)
                .ok_or_else(|| state_error("read", "recovery key has no prefix"))?
                .to_vec();
            records.push((suffix, object));
            if bytes >= max_bytes {
                break iter
                    .next()
                    .await
                    .map_err(|error| state_error("read recovery page", error))?
                    .is_none();
            }
        };
        Ok(LedgerPage { records, end })
    }

    pub(crate) async fn state(&self) -> Result<Option<RecoveryCursor>> {
        self.view
            .get(STATE_KEY)
            .await
            .map_err(|error| state_error("read state", error))?
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()
            .map_err(|error| state_error("decode state", error))
    }

    pub(crate) async fn set_state(&self, cursor: &RecoveryCursor) -> Result<()> {
        let bytes =
            serde_json::to_vec(cursor).map_err(|error| state_error("encode state", error))?;
        self.view
            .set(STATE_KEY, &bytes)
            .await
            .map_err(|error| state_error("write state", error))
    }
}

#[cfg(test)]
mod tests;
