use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query, db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A relation commits to its target's collection ID whether the target
        // was declared in the same SDL or is already stored, so splitting a
        // schema over several calls moves no collection ID.
        family: "Relation target identity",
        name: "added_alone_equals_added_together / ids_depend_on_what_is_seen / stored_target_is_committed / declared_namesake_stays_named: a collection added after its relation target has the ID it has when both are added in one call",
        axis: Lean,
        anchor: "crates/engine/query/src/sdl_parse/builder_field_kinds.rs resolve_field_kind; crates/engine/query/src/sdl_parse/builder.rs build_collections; crates/engine/query/src/sdl_parse/parser.rs parse_sdl_with_known_collections; crates/database/db/src/collection/ops/add/mod.rs known_collections, prepare_new_collections; crates/database/db/src/txn/registry/schemas.rs DbTransactionRegistry::add_collection",
        model_ref: "RelationTargetIdentity (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/sdl_parse/known_collection_tests.rs a_relation_to_a_known_collection_commits_to_its_id, a_chain_parsed_in_three_steps, a_type_of_the_sdl_wins_over_a_known_namesake; crates/database/db/tests/collection/relation_identity.rs a_relation_to_a_stored_collection, a_chain_added_in_three_calls, a_collection_added_in_a_transaction_after_its_target; tools/integration-test/tests/testo/collection/id_added_alone.rs a_collection_added_after_its_target_has_the_id_of_one_call",
    },
    Property {
        // An input object may hold only input types, so every relation kind,
        // one left with only its target's name included, is an ID there.
        family: "Relation target identity",
        name: "mutation_input_is_always_an_input_type: no field kind reaches a mutation input as an object type",
        axis: Lean,
        anchor: "crates/engine/query/src/runner/introspection/input_types.rs field_kind_to_input_type_ref, build_mutation_input_type",
        model_ref: "RelationTargetIdentity (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/query/src/runner/introspection/named_relation_tests.rs a_named_relation_is_an_id_in_a_mutation_input, a_named_relation_to_many_is_an_id_too; crates/entry-points/defra-node/tests/relation_identity.rs a_stored_named_relation_keeps_its_identity_and_the_node_its_schema",
    },
];
