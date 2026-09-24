use std::sync::Arc;

use async_trait::async_trait;
use cid::Cid;
use defra_core::block::{Block, CollectionDefinitionDeltaPayload, CompositeDeltaPayload};
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
/// A definition block of a governed collection, as the merge path holds it
/// before the version it describes is stored.
///
/// The identity is self-certifying for an initial definition: the collection
/// ID commits to the root, so a definition claiming a root with other fields
/// is a different collection, not a forgery of this one. A patch is not: it
/// inherits the collection ID and changes what every later write is judged
/// against, so who may publish one is the application's rule to state.
pub struct DefinitionCandidate<'a> {
    /// The version ID: the CID of this block.
    pub cid: &'a Cid,
    pub block: &'a Block,
    pub payload: &'a CollectionDefinitionDeltaPayload,
    /// The version as this node has rebuilt it from the block: the record
    /// that will be stored if the verdict accepts.
    pub version: &'a CollectionVersion,
    /// The version this block patches, as held here; `None` for an initial
    /// definition.
    pub previous: Option<&'a CollectionVersion>,
}

impl DefinitionCandidate<'_> {
    /// Whether this is the collection's first definition rather than a patch.
    pub fn is_initial(&self) -> bool {
        self.previous.is_none()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MergeValidator: MaybeSendSync {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String>;

    /// Judge a definition block of a governed collection the app has claimed,
    /// before the version it describes is stored. The same three verdicts
    /// and the same rules as [`Self::validate`]: a reject rests on the
    /// block's own content, a defer names what could change it, and the
    /// verdict reads nothing but held bytes through `view`.
    ///
    /// Not called for an ungoverned collection, nor for a version this node
    /// already holds. A rejected definition is left unmerged and never
    /// stored; a deferred one is re-driven when what it awaits merges.
    ///
    /// `candidate.version.governance_rule` is the rule tag the version
    /// commits to. A validator that judges under one rule should refuse a
    /// version naming another it does not recognise, so a rule change is an
    /// upgrade this node either follows or declines, never one it judges
    /// wrongly.
    ///
    /// The default accepts every definition, which is the behaviour before
    /// this entry point existed: a version arriving over the network is
    /// stored inactive for an operator to activate.
    async fn validate_definition(
        &self,
        candidate: &DefinitionCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let _ = (candidate, view);
        Ok(MergeVerdict::Accept)
    }
}

/// The collections an app has claimed and the validator that governs them.
///
/// A claimed collection is judged only by the validator: the ACP merge hook
/// does not run for it, and a claimed collection with no validator installed
/// defers every composite rather than merging it as ungoverned.
#[derive(Clone, Default)]
pub struct MergeGovernance {
    collections: RapidHashSet<String>,
    validator: Option<Arc<dyn MergeValidator>>,
}

impl MergeGovernance {
    /// Claim collections by name.
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

    pub fn governs(&self, collection: &CollectionVersion) -> bool {
        self.collections.contains(&collection.name)
    }

    pub fn validator(&self) -> Option<&Arc<dyn MergeValidator>> {
        self.validator.as_ref()
    }
}
