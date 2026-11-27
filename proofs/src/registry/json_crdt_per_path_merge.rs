use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "crdt, db-merge (specified before the code, #2129)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The JSON CRDT (#2129). Each path is a register: racing edits resolve
        // by priority, then block ID; an edit wipes only what it had seen below
        // it, so an update beats a concurrent delete; only a set below a path
        // makes it a map, so a delete creates nothing; the conflicts query
        // lists exactly the racers that lost. The defra-core tests check the
        // same invariants on block DAGs; the integration tests merge real
        // blocks between two nodes.
        family: "JSON CRDT per-path merge",
        name: "INV_Converge / INV_MatchesReplay / INV_DisjointKept / INV_SamePathLww / INV_UpdateWins / INV_TypeClash / INV_ConflictsAreLosers / INV_DeleteCreatesNothing / INV_WriteLandsAsRead: per-path JSON merge converges, keeps every edit nothing saw, never lets a delete create a path, keeps two signers' equal edits apart by their nonces, and lands every write, several paths at once included, as its writer reads it",
        axis: Tla,
        anchor: "crates/foundation/defra-core/src/json_crdt.rs Frontier::delta, Frontier::compose, Frontier::materialize, Frontier::conflicts, replay; crates/building-blocks/crdt/src/field/json.rs JsonField (TaggedCrdt: write_data, effect, replay), block_ops, settle_earlier, apply_ops, landed, mismatch; crates/foundation/defra-core/src/json_crdt.rs encode_write, decode_ops; crates/building-blocks/crdt/src/field/tagged.rs local_write, land; crates/database/db/src/merge/merge_handler/field_tagged.rs merge_field_tagged, tagged_past; crates/database/db/src/write/autocommit/helpers.rs prepare_tagged_writes; crates/database/db/src/definition/migration/helpers.rs changes_block_state; crates/foundation/defra-core/src/json_crdt_tests.rs, crates/building-blocks/crdt/tests/json_field.rs, crates/database/db/tests/merge/json_merge.rs, tools/integration-test/tests/legacy/p2p/json_crdt.rs",
        model_ref: "MC_JsonCrdt_{Green_FirstValueOrMap,Green_FirstListOrDelete,Green_MultiPath,Red_DeleteWins,Red_NoCid,Red_DeleteImpliesMap,Red_SharedId,Red_NoAnchor,Red_NoSettle,Red_DeleteMakesParent,Red_MapKeepsBelow}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/json_merge.rs equal_concurrent_json_edits_from_two_signers_converge_in_any_order; crates/building-blocks/crdt/tests/json_field.rs json_meets_the_contract; crates/foundation/defra-core/src/json_crdt_tests.rs disjoint_keys_both_survive_in_any_order, an_update_beats_a_concurrent_delete_above_it",
    },
    Property {
        // A path's register keeps the same winner whatever order edits arrive
        // in, or if one arrives twice; a path's key reads back as the path, and
        // one key starts another only when its path is above the other's; the
        // keys under a path are one stretch of the frontier's sorted entries.
        family: "JSON CRDT per-path merge",
        name: "join_comm / join_assoc / join_idem / decode_encode / encode_prefix / prefix_convex: order-free registers, nesting path keys, and one range per subtree",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/json_crdt.rs beats, encode_path, decode_path, Frontier::under, Frontier::at",
        model_ref: "JsonCrdt.{Lww,Path} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/foundation/defra-core/src/json_crdt_tests.rs diff_then_merge_gives_the_new_value, disjoint_keys_both_survive_in_any_order, one_path_resolves_by_priority_then_block_and_lists_the_loser",
    },
];
