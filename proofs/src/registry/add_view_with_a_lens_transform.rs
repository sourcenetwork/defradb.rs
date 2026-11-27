use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-node, db, schema, lens (#2724; NAC before the create transaction is a known gap, #2736)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2724: add_view accepts a transform CID only if it is non-empty and
        // held, stores it on the view's query source, and the definition's
        // JSON reads it back unchanged.
        family: "Add view with a lens transform",
        name: "ViewQuerySource.round_trip / definition_round_trip / stored_transform_is_accepted",
        axis: Lean,
        anchor: "crates/foundation/schema/src/source.rs QuerySource::with_transform; crates/foundation/schema/src/collection.rs CollectionVersion; crates/database/db/src/view/client/mod.rs DbTransactionRegistry::add_view, view_source; crates/database/db/src/collection/ops/create.rs DB::require_view_transforms",
        model_ref: "ViewQuerySource (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2724: the NAC, empty-CID and lens checks against the view's commit,
        // while the lens arrives (added locally or fetched by P2P sync) and
        // the permission changes. Both arrivals commit the lens config, and
        // the create transaction reads it (require_view_transforms);
        // NAC ViewAdd is still read before it, a known gap (#2736):
        // Green_NacBefore and Red_KnownGap_NacBefore.
        family: "Add view with a lens transform",
        name: "INV_NoEmptyTransform / INV_StoredTransformExists / INV_StoredIsAccepted / INV_NoViewWithoutPermission / INV_NoValidRejected",
        axis: Tla,
        anchor: "crates/database/db/src/view/client/mod.rs DbTransactionRegistry::add_view, view_source; crates/database/db/src/collection/ops/create.rs DB::create_view_collections_as, DB::stage_new_collections, DB::require_view_transforms; crates/database/db/src/definition/migration/committed_lenses.rs DB::add_lens, DB::commit_synced_lens, write_lens_config; crates/surfaces/p2p-adapter/src/version_syncer.rs sync_lens; crates/surfaces/p2p-adapter/src/transport_version_syncer.rs sync_lens",
        model_ref: "MC_AddViewTransform_{Green,Green_NacBefore,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // #2802: a materialized view's cache selects every plain field the
        // view declares, fields only its lens produces included, and nothing
        // the view does not declare.
        family: "Add view with a lens transform",
        name: "ViewCacheSelection.plain_field_selected / selected_is_declared",
        axis: Lean,
        anchor: "crates/database/db/src/view/selection.rs cache_selection; crates/database/db/src/view/ops.rs DB::build_view_cache",
        model_ref: "ViewCacheSelection (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/view/simple/with_unique_index.rs view_materialized_transform_giving_two_items_one_unique_value",
    },
];
