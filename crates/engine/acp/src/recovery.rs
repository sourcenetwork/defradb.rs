//! The node's own confirmed access state, for recovering a Vera deployment.
//!
//! Every registration and relationship change the provider confirmed leaves
//! one fact here, under a key of its own: an object's owner, or one
//! relationship as held or deleted. A fact is a blind put in the transaction
//! that settles its record in the outbox, so it reads nothing and conflicts
//! with nothing. A change sent to the provider outside a transaction has no
//! outbox record, so its fact is first written as an intent beside the fact
//! and cleared once the provider has answered. An intent left without an
//! answer is not proof: a pass owes a delete left behind, which wins the
//! slot, and only reports a grant or registration left behind, which never
//! outranks what the provider confirmed. The next answer the provider gives
//! for the slot replaces both.

use async_trait::async_trait;
use datastore::NamespaceView;
use identity::Did;
use serde::{Deserialize, Serialize};
use storage::corekv::IterOptions;

use crate::error::{Error, Result};
use crate::outbox::OwedSubject;

/// The start of every recovery record in the ACP namespace.
pub(crate) const RECOVERY_PREFIX: &[u8] = b"/acp-recovery/object/";
const STATE_KEY: &[u8] = b"/acp-recovery/state";

/// A page reads at most this many encoded bytes of records, and the records
/// of the slot it stopped in.
pub const RECOVERY_PAGE_BYTES: usize = 256 * 1024;

const OWNER_SLOT: u8 = 0;
const RELATIONSHIP_SLOT: u8 = 1;

const CONFIRMED: u8 = 0;
const INTENT_DENYING: u8 = 1;
const INTENT_ALLOWING: u8 = 2;

/// What the provider holds in one slot of an object, as this node last
/// confirmed it or is about to ask for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoveryState {
    /// The object's registration: its owner, or none once unregistered.
    Owner { owner: Option<Did> },
    /// One relationship, held or deleted, and the actor that made it so.
    Relationship {
        actor: Did,
        relation: String,
        subject: OwedSubject,
        held: bool,
    },
}

impl RecoveryState {
    /// Whether the state takes access away: an unregistration or a delete.
    fn denies(&self) -> bool {
        match self {
            Self::Owner { owner } => owner.is_none(),
            Self::Relationship { held, .. } => !held,
        }
    }
}

/// One slot of one object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryFact {
    pub policy_id: String,
    pub resource_name: String,
    pub object_id: String,
    #[serde(flatten)]
    pub state: RecoveryState,
}

impl RecoveryFact {
    pub fn registered(policy_id: &str, resource_name: &str, object_id: &str, owner: Did) -> Self {
        Self::owner(policy_id, resource_name, object_id, Some(owner))
    }

    pub fn unregistered(policy_id: &str, resource_name: &str, object_id: &str) -> Self {
        Self::owner(policy_id, resource_name, object_id, None)
    }

    fn owner(policy_id: &str, resource_name: &str, object_id: &str, owner: Option<Did>) -> Self {
        Self {
            policy_id: policy_id.to_owned(),
            resource_name: resource_name.to_owned(),
            object_id: object_id.to_owned(),
            state: RecoveryState::Owner { owner },
        }
    }

    pub fn relationship(
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        actor: Did,
        relation: &str,
        subject: OwedSubject,
        held: bool,
    ) -> Self {
        Self {
            policy_id: policy_id.to_owned(),
            resource_name: resource_name.to_owned(),
            object_id: object_id.to_owned(),
            state: RecoveryState::Relationship {
                actor,
                relation: relation.to_owned(),
                subject,
                held,
            },
        }
    }

    pub(crate) fn same_object(&self, other: &Self) -> bool {
        self.policy_id == other.policy_id
            && self.resource_name == other.resource_name
            && self.object_id == other.object_id
    }

    fn same_slot(&self, other: &Self) -> bool {
        self.same_object(other)
            && match (&self.state, &other.state) {
                (RecoveryState::Owner { .. }, RecoveryState::Owner { .. }) => true,
                (
                    RecoveryState::Relationship {
                        relation, subject, ..
                    },
                    RecoveryState::Relationship {
                        relation: other_relation,
                        subject: other_subject,
                        ..
                    },
                ) => relation == other_relation && subject == other_subject,
                _ => false,
            }
    }

    /// The slot's key without its last byte, which tells a confirmed fact
    /// from an intent. An object's owner slot sorts before its relationships.
    fn slot_key(&self) -> Result<Vec<u8>> {
        let mut key = object_prefix(&self.policy_id, &self.resource_name, &self.object_id);
        match &self.state {
            RecoveryState::Owner { .. } => key.push(OWNER_SLOT),
            RecoveryState::Relationship {
                relation, subject, ..
            } => {
                key.push(RELATIONSHIP_SLOT);
                let subject =
                    serde_json::to_vec(subject).map_err(|error| state_error("encode", error))?;
                for part in [relation.as_bytes(), subject.as_slice()] {
                    key.extend_from_slice(&(part.len() as u64).to_be_bytes());
                    key.extend_from_slice(part);
                }
            }
        }
        Ok(key)
    }

    fn key(&self, record: u8) -> Result<Vec<u8>> {
        let mut key = self.slot_key()?;
        key.push(record);
        Ok(key)
    }

    fn intent_key(&self) -> Result<Vec<u8>> {
        self.key(if self.state.denies() {
            INTENT_DENYING
        } else {
            INTENT_ALLOWING
        })
    }

    /// What a transaction writes to record this fact as the provider
    /// confirmed it.
    pub(crate) fn confirmed(&self) -> Result<Confirmation> {
        Ok(Confirmation {
            key: self.key(CONFIRMED)?,
            value: self.encode()?,
            unanswered: [self.key(INTENT_DENYING)?, self.key(INTENT_ALLOWING)?],
        })
    }

    fn encode(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|error| state_error("encode", error))
    }
}

/// The writes that record a confirmed fact: one put, and the deletes of
/// what was unanswered in its slot.
pub(crate) struct Confirmation {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub unanswered: [Vec<u8>; 2],
}

fn object_prefix(policy_id: &str, resource_name: &str, object_id: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(
        RECOVERY_PREFIX.len() + 64 + policy_id.len() + resource_name.len() + object_id.len(),
    );
    key.extend_from_slice(RECOVERY_PREFIX);
    for part in [policy_id, resource_name, object_id] {
        key.extend_from_slice(&(part.len() as u64).to_be_bytes());
        key.extend_from_slice(part.as_bytes());
    }
    key
}

/// Where a recovery pass stopped: the term it checks and the last record it
/// handed over. `complete` once it has handed over the last record there is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecoveryCursor {
    pub term: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Vec<u8>>,
    #[serde(default)]
    pub complete: bool,
}

/// What one bounded page of a pass found.
#[derive(Debug, Default)]
pub struct RecoveryPage {
    pub effects: Vec<crate::VoidedEffect>,
    pub cursor: RecoveryCursor,
    /// What this page could not prove or restore. It is reported, never dropped.
    pub unrestored: Vec<String>,
}

/// What the deployment says of one relationship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observed {
    Held,
    Absent,
    /// The deployment cannot be asked about it, and why.
    Unobservable(String),
}

/// The deployment, as only a recovery pass reads it.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RecoveryVerifier: defra_core::thread_bounds::MaybeSendSync {
    /// The certified term the pass is rebuilding.
    async fn term(&self) -> std::result::Result<u64, String>;

    /// The object's owner under `term`, when it is registered.
    async fn object_owner(
        &self,
        term: u64,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
    ) -> std::result::Result<Option<String>, String>;

    /// Whether `subject` holds `relation` on the object under `term`, read
    /// as the object's `owner`.
    #[allow(clippy::too_many_arguments)]
    async fn relationship(
        &self,
        term: u64,
        owner: &Did,
        policy_id: &str,
        resource_name: &str,
        object_id: &str,
        relation: &str,
        subject: &OwedSubject,
    ) -> std::result::Result<Observed, String>;
}

/// One step of a pass.
#[derive(Debug)]
pub enum RecoveryStep {
    /// A page to hand to the outbox and then record as delivered.
    Page(RecoveryPage),
    /// The whole pass for `term` has been handed over.
    Done { term: u64 },
}

/// The node's own access state, read in bounded resumable pages.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RecoverySource: defra_core::thread_bounds::MaybeSendSync {
    /// The next page of the pass for the verifier's term, or the term whose
    /// pass has been handed over whole. A cursor of an older term is not
    /// resumed: the pass starts again.
    async fn next(&self, verifier: &dyn RecoveryVerifier) -> Result<RecoveryStep>;

    /// Record that the page ending at `cursor` reached the outbox.
    async fn delivered(&self, cursor: RecoveryCursor) -> Result<()>;
}

fn state_error(what: &str, source: impl std::fmt::Display) -> Error {
    Error::Storage(format!("Vera recovery {what}: {source}"))
}

/// A slot as a pass reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    /// The state a pass holds the deployment to.
    pub fact: RecoveryFact,
    /// Whether the provider confirmed that state. A change it never answered
    /// is not confirmed: a pass acts on it only where that takes access away.
    pub confirmed: bool,
    /// A grant or registration the provider never answered, beside a state
    /// it did confirm. It changes nothing a pass does and is reported.
    pub unanswered: Option<RecoveryFact>,
}

/// The state a slot's records come to. An unanswered change that takes
/// access away wins. Otherwise what the provider confirmed stands, and an
/// unanswered change that gives access stands only where nothing was
/// confirmed.
fn resolve(records: [Option<RecoveryFact>; 3]) -> Option<Slot> {
    let [confirmed, denying, allowing] = records;
    if let Some(fact) = denying {
        return Some(Slot {
            confirmed: confirmed.as_ref() == Some(&fact),
            fact,
            unanswered: None,
        });
    }
    match confirmed {
        Some(fact) => Some(Slot {
            unanswered: allowing.filter(|asked| *asked != fact),
            fact,
            confirmed: true,
        }),
        None => allowing.map(|fact| Slot {
            fact,
            confirmed: false,
            unanswered: None,
        }),
    }
}

/// One page of slots, each resolved, with the key of the last record read.
pub(crate) struct LedgerPage {
    pub slots: Vec<Slot>,
    pub last: Option<Vec<u8>>,
    /// Whether the page reached the end of the ledger.
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

    async fn put(&self, key: &[u8], fact: &RecoveryFact) -> Result<()> {
        self.view
            .set(key, &fact.encode()?)
            .await
            .map_err(|error| state_error("write", error))
    }

    async fn read(&self, key: &[u8]) -> Result<Option<RecoveryFact>> {
        self.view
            .get(key)
            .await
            .map_err(|error| state_error("read", error))?
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()
            .map_err(|error| state_error("decode", error))
    }

    /// Record what the provider confirmed. Its answer is the slot's newest
    /// word, so a change an earlier stop left unanswered is dropped.
    pub(crate) async fn confirm(&self, fact: &RecoveryFact) -> Result<()> {
        self.put(&fact.key(CONFIRMED)?, fact).await?;
        for intent in [INTENT_DENYING, INTENT_ALLOWING] {
            self.clear(&fact.key(intent)?).await?;
        }
        Ok(())
    }

    async fn clear(&self, key: &[u8]) -> Result<()> {
        self.view
            .delete(key)
            .await
            .map_err(|error| state_error("write", error))
    }

    /// Record that `fact` is about to be asked of the provider.
    pub(crate) async fn prepare(&self, fact: &RecoveryFact) -> Result<()> {
        self.put(&fact.intent_key()?, fact).await
    }

    /// Record the provider's answer to `fact`: confirmed when it applied,
    /// and its intent cleared either way.
    pub(crate) async fn settle(&self, fact: &RecoveryFact, applied: bool) -> Result<()> {
        if applied {
            return self.confirm(fact).await;
        }
        self.clear(&fact.intent_key()?).await
    }

    /// The owner slot of `fact`'s object, resolved.
    pub(crate) async fn owner_of(&self, fact: &RecoveryFact) -> Result<Option<Slot>> {
        let slot =
            RecoveryFact::unregistered(&fact.policy_id, &fact.resource_name, &fact.object_id);
        Ok(resolve([
            self.read(&slot.key(CONFIRMED)?).await?,
            self.read(&slot.key(INTENT_DENYING)?).await?,
            self.read(&slot.key(INTENT_ALLOWING)?).await?,
        ]))
    }

    /// The slots after the record `after`, until their encoded bytes pass
    /// `max_bytes`. A slot's records are never split between two pages.
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
        let mut slots = Vec::new();
        let mut last = None;
        let mut group: [Option<RecoveryFact>; 3] = [None, None, None];
        let mut bytes = 0;
        let end = loop {
            let Some(entry) = iter
                .next()
                .await
                .map_err(|error| state_error("read recovery page", error))?
            else {
                break true;
            };
            let fact: RecoveryFact = serde_json::from_slice(&entry.value)
                .map_err(|error| state_error("decode recovery page", error))?;
            let record = match entry.key.last() {
                Some(&record) if record <= INTENT_ALLOWING => usize::from(record),
                _ => return Err(state_error("read", "recovery key names no record")),
            };
            let open = group.iter().flatten().next();
            if open.is_some_and(|open| !open.same_slot(&fact)) {
                slots.extend(resolve(std::mem::take(&mut group)));
                if bytes >= max_bytes {
                    break false;
                }
            }
            bytes += entry.value.len();
            last = Some(
                entry
                    .key
                    .strip_prefix(RECOVERY_PREFIX)
                    .ok_or_else(|| state_error("read", "recovery key has no prefix"))?
                    .to_vec(),
            );
            group[record] = Some(fact);
        };
        slots.extend(resolve(group));
        Ok(LedgerPage { slots, last, end })
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
