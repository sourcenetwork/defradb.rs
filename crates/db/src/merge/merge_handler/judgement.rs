//! The seam between the merge path and a merge validator. The merge path
//! hands the judge one frame and acts on its judgement; with
//! `governed-collections` the judge is `governance::judge`, without it
//! `ungoverned` answers for every frame.

use cid::Cid;
use defra_core::block::{Block, CompositeDeltaPayload};
use defra_core::merge::{BlockMetadata, MergeOutcome};
use schema::CollectionVersion;

/// One composite frame as the merge path hands it to the judge.
#[cfg_attr(not(feature = "governed-collections"), allow(dead_code))]
pub(crate) struct GovernedFrame<'a> {
    /// The composite the merge was asked for, re-driven if this frame defers.
    pub root: &'a Cid,
    pub cid: &'a Cid,
    pub block: &'a Block,
    pub payload: &'a CompositeDeltaPayload,
    /// What carried the root, kept with a deferral for its re-drive.
    pub metadata: &'a BlockMetadata<'a>,
    pub doc_id: &'a str,
    pub collection: &'a CollectionVersion,
}

/// What the judge said about a frame. A deferral is already filed for
/// re-drive by the time it is returned.
#[cfg_attr(not(feature = "governed-collections"), allow(dead_code))]
pub(crate) enum Judgement {
    /// No validator governs the collection: ACP decides.
    Ungoverned,
    /// The validator accepted; ACP does not run for a governed collection.
    Accept,
    /// The validator rejected or deferred; the merge ends with this outcome.
    Settled(MergeOutcome),
}
