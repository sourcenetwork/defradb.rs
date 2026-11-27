use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use defra_core::block::{Block, CompositeDeltaPayload};
use defra_core::thread_bounds::MaybeSendSync;
use rapidhash::RapidHashSet;
use schema::CollectionVersion;

use super::signature::SignatureStatus;
use super::verdict::MergeVerdict;
use super::view::MergeView;

/// One composite arriving over replication into a governed collection.
///
/// Every composite is judged on its own: an ancestor loaded from the
/// blockstore, a composite linked from a collection block and a genesis
/// composite each reach the validator as a separate candidate.
pub struct MergeCandidate<'a> {
    pub cid: &'a Cid,
    pub block: &'a Block,
    pub payload: &'a CompositeDeltaPayload,
    /// The document the composite belongs to, derived from its DAG.
    pub doc_id: &'a str,
    /// The local collection resolved from the composite's own schema version.
    pub collection: &'a CollectionVersion,
    pub is_genesis: bool,
    pub signature: SignatureStatus,
}

/// Decides whether a replicated composite in a governed collection merges.
///
/// # Purity contract
///
/// The verdict is replicated behaviour: every honest node running the same
/// validator must reach the same verdict on the same composite, whatever order
/// blocks arrived in, or replicas diverge permanently. A validator must
/// therefore be a function of the candidate and of what it reads through the
/// [`MergeView`] only. It must not read the wall clock, randomness, the node's
/// own identity, ACP registration state, transport peers, or any state kept
/// beside the database.
///
/// - Return [`MergeVerdict::Reject`] only for a composite that is invalid on
///   its own content and will never be accepted on replay.
/// - Return [`MergeVerdict::Defer`] when an input is missing, naming the CIDs
///   whose arrival could change the verdict. Absence of an input is never a
///   reason to reject.
/// - Return `Err` for a transient local failure. The host leaves the
///   composite unmerged and never treats an error as a verdict.
///
/// Replication policy is node-local and is never an input to this verdict.
///
/// # Which collections it is asked about
///
/// Once installed, a validator is asked about every collection whose
/// definition declares a governance root (`@governed(root:)`) and carries no
/// `@policy`, including one defined after the node started, as well as the
/// collections claimed by name in [`MergeGovernance::new`]. A validator with no rule for such a collection
/// still has to decide: return [`MergeVerdict::Accept`] if the collection is
/// meant to replicate unjudged, and [`MergeVerdict::Reject`] if it is not.
/// [`MergeVerdict::Defer`] is for a verdict that a named input will settle. A
/// deferred composite stays unmerged and keeps its sender's pending-DAG
/// record, so a validator that defers a whole collection fills that peer's
/// share and the node stops taking the peer's other documents.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MergeValidator: MaybeSendSync {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String>;
}

/// The validator that judges governed collections, and the collections an
/// app has claimed for it by name.
///
/// A collection is governed when its name was claimed here, or when a
/// validator is installed and the collection's definition declares a
/// governance root (`@governed(root:)`) and carries no `@policy`. With no
/// validator installed only the claimed names are governed, so a collection
/// that declares a root merges as any other collection does.
///
/// ACP takes precedence over a declared root: a collection that carries
/// `@policy` is governed only when its name was claimed, so a rooted one
/// that was not claimed keeps the whole ACP merge hook and is not judged.
///
/// The validator decides whether a replicated composite of a governed
/// collection merges. The ACP merge hook's per-collection steps that the
/// merge path gates on governance (replica registration, the protected
/// composite and encrypted link checks, the post-commit action) do not run
/// for it; a hook that checks protected updates inside the merge transaction
/// still does. A claimed collection with no validator installed holds every
/// replicated composite unmerged, as a retryable skip, rather than merging it
/// as ungoverned.
///
/// Limitations: a `@chunk` collection generated for a governed parent
/// declares no root of its own, so it is governed only when claimed by name.
/// A collection that carries `@policy` cannot have both: claimed by name it
/// is judged and loses the ACP merge hook steps above; not claimed it keeps
/// them and its declared root is not honoured.
#[derive(Clone, Default)]
pub struct MergeGovernance {
    collections: RapidHashSet<String>,
    validator: Option<Arc<dyn MergeValidator>>,
}

impl MergeGovernance {
    /// Claim collections by name. A validator installed with
    /// [`with_validator`](Self::with_validator) also governs every collection
    /// that declares a governance root and carries no `@policy`, named or not.
    pub fn new(collections: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            collections: collections.into_iter().map(Into::into).collect(),
            validator: None,
        }
    }

    pub fn with_validator(mut self, validator: Arc<dyn MergeValidator>) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Whether the validator decides `collection`'s replicated composites:
    /// its name was claimed, or a validator is installed and it declares a
    /// governance root and carries no `@policy`.
    pub fn governs(&self, collection: &CollectionVersion) -> bool {
        self.collections.contains(&collection.name)
            || (self.validator.is_some()
                && collection.governance_root.is_some()
                && collection.policy.is_none())
    }

    pub fn validator(&self) -> Option<&Arc<dyn MergeValidator>> {
        self.validator.as_ref()
    }
}
