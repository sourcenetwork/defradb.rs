use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "prolly, storage";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Node contents are not modelled: a node is named by its hash, so a
        // cached or pending copy is never stale. One worker owns the store,
        // so a flush and a read never overlap. The Rust cache's CLOCK victim
        // choice is one of the model's "any part that fits".
        family: "Prolly tree node store and cache",
        name: "GetFinds / CacheBounded / HugeEvictsNothing / PublishedStored / BuildWhole: a node put is always readable, the cache stays under its byte budget, a node over budget evicts nothing, a published root names only stored nodes, and a build whose put failed never returns a tree",
        axis: Tla,
        anchor: "crates/foundation/prolly/src/cache.rs Cached::get, Cached::insert; crates/foundation/prolly/src/tree.rs collapse; crates/engine/storage/src/stores/prolly_nodes.rs ProllyNodes::get, ProllyNodes::put, ProllyNodes::flush; crates/foundation/prolly/src/build.rs Builder::push, Builder::finish",
        model_ref: "MC_ProllyNodeCache_Green.cfg (GREEN) / MC_ProllyNodeCache_{Red_SkipPending,Red_NoEvict,Red_CacheHuge,Red_DropOnFail,Red_PublishBeforeFlush,Red_FinishAfterFail}.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A sweep's candidates are one bounded page. The delete transaction
        // reads the live roots itself, validated, so a root published before
        // its commit re-runs it. The model trusts that `LiveRoots::read`
        // returns every live root, and that the caller storing the roots
        // publishes one in the transaction that writes its nodes: nothing in
        // the workspace publishes a root yet.
        family: "Prolly tree node store and cache",
        name: "SweepKeepsLiveRoots: a page sweep racing a root's publication never deletes a node the live root names",
        axis: Tla,
        anchor: "crates/engine/storage/src/stores/prolly_nodes.rs ProllyNodes::reclaim, node_keys, ReclaimNodes::run, mark_roots, LiveRoots::read",
        model_ref: "MC_ProllyNodeReclaim_Green.cfg (GREEN) / MC_ProllyNodeReclaim_{Red_FixedRoots,Red_NoValidation,Red_PutAlone,Red_DeleteMarked}.cfg (RED)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The data rule the delete applies: a node is deleted exactly when
        // it is on the page and the live marks do not name it, so a marked
        // node never is.
        family: "Prolly tree node store and cache",
        name: "reclaimDeletesOnlyUnmarked / reclaimKeepsMarked",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/prolly_nodes.rs ReclaimNodes::run, mark_roots",
        model_ref: "ProllyTree (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
