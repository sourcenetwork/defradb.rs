use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Custom CRDT types (#2183). A node without a registered type merges
        // the rest of each block, keeps the custom block and a record of it,
        // and acknowledges it; built again with the type, it merges each
        // parked block once, removing its record in the same transaction. A
        // block whose only part was parked still makes the document, and a
        // replay that fails for now (a store error, a key this node does not
        // hold) leaves its block parked. A node storing a value of a type
        // refuses to start without it. The defra-node custom_crdt tests run
        // the same story between real nodes.
        family: "Custom CRDT types",
        name: "INV_NoBlindMerge / INV_MergeOnce / INV_OtherFieldsExact / INV_ConvergeWhenDone / LIVE_Converge: an unregistered type is never merged, a parked block merges exactly once, other fields stay exact, and replicas converge once the type is registered",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/field/registry.rs Crdts::delta, Crdts::field, Crdts::part; crates/database/db/src/merge/merge_handler/field.rs merge_field_delta_in_txn; crates/database/db/src/merge/merge_handler/composite_fields.rs process_linked_field_blocks; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/database/db/src/merge/merge_handler/parked.rs park, replay_parked, replay_in_txn, DB::start_parked_replay; crates/database/db/src/merge/merge_handler/error.rs MergeError::fails_block, MergeError::needs_key; crates/database/db/src/merge/merge_handler/merge_keys.rs with_local_history_keys; crates/database/db/src/database/stored_crdts.rs record, DB::check_stored_crdts; crates/entry-points/defra-node/tests/custom_crdt/replication.rs, crates/database/db/tests/merge/parked_errors.rs",
        model_ref: "MC_UnknownCrdt_{Green,Red_DropDeferral,Red_DoubleReplay,Red_Lww,Red_RemoveStored,Red_NoDocument,Red_FailStall}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/custom_crdt/replication.rs a_node_without_the_type_parks_its_blocks_until_it_has_the_type; crates/database/db/tests/merge/parked_errors.rs a_parked_block_fails_only_for_an_error_of_its_own",
    },
    Property {
        // Every existing type keeps its definition bytes (code, no name), so
        // definition CIDs do not move; a custom type is code 8 plus its name;
        // reading back what a type writes gives that type. The defra-node
        // definition_cids golden test pins the CIDs themselves.
        family: "Custom CRDT types",
        name: "native_unchanged / custom_not_native / decode_encode / encode_injective / old_codes_read_back: custom types leave native definitions byte-identical and round-trip",
        axis: Lean,
        anchor: "crates/foundation/schema/src/ctype.rs CType::to_u8, CType::custom_name, CType::from_definition; crates/foundation/schema/src/cid.rs field_to_delta_with_priority; crates/foundation/defra-core/src/block_delta.rs FieldDefinitionDeltaPayload; crates/entry-points/defra-node/tests/definition_cids.rs",
        model_ref: "CustomCrdtCode (lake build)",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/definition_cids.rs native_definitions_keep_their_ids",
    },
];
