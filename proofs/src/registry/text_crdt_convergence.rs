use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "crdt, db-merge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Peers that merged the same blocks show the same text, a peer's text
        // built one block at a time equals a replay from scratch, every edit
        // lands at its position in its own parent version, and a run typed at
        // one spot is never split by a concurrent one; a write of null reads
        // null exactly while every head writes null. The crdt tests check the
        // same invariants on random DAGs; the db tests merge real blocks
        // between two nodes.
        family: "Text CRDT convergence",
        name: "INV_Converge / INV_MatchesReplay / INV_Intent / INV_PlacedAsIntended / INV_NoInterleave / INV_CriticalReplay / INV_NullConverge / INV_NullMatchesReplay / INV_NullEmpty: Fugue text converges, keeps every edit where its author put it, never interleaves runs, replays a concurrent merge from its critical block, and reads null only when every head writes null",
        axis: Tla,
        anchor: "crates/building-blocks/crdt/src/fugue.rs Tree::apply, Tree::attach, replay, merge_text; crates/building-blocks/crdt/src/field/text.rs TextField (block_data, CausalCrdt diff, block_op, replay, merge_from, check_version, nil_heads); crates/database/db/src/merge/merge_handler/field_causal.rs merge_field_causal, critical_block, causal_history; crates/database/db/src/write/autocommit/helpers.rs apply_field_ops; crates/database/db/src/definition/migration/helpers.rs changes_block_state; crates/building-blocks/crdt/src/fugue_tests.rs, crates/building-blocks/crdt/src/field/testing.rs check_causal, crates/database/db/tests/merge/text_merge.rs",
        model_ref: "MC_TextCrdt_{Green,Red_ArrivalOrder,Red_NoCausalBuffer,Red_CurrentText,Red_NextNode,Red_CommonAncestor,Red_NullLast}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/merge/text_merge.rs concurrent_edits_of_one_text_keep_both_on_every_node; crates/building-blocks/crdt/src/fugue_tests.rs concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, an_edit_concurrent_with_a_delete_still_finds_its_spot",
    },
    Property {
        // Fugue's rule puts an inserted character right after its left origin
        // and moves nothing else; a delete hides only its character; two
        // concurrent characters hung in either order give the same tree; a
        // placeholder run kept as one span and cut where an edit lands is the
        // same tree as its chain of characters.
        family: "Text CRDT convergence",
        name: "place_visible / delete_visible / attach_comm / split_run: intention preservation and arrival-order independence of the Fugue tree, and span cuts that change nothing",
        axis: Lean,
        anchor: "crates/building-blocks/crdt/src/fugue.rs Tree::apply, Tree::attach, Tree::traverse, Tree::split",
        model_ref: "TextCrdt.{Fugue,Span} (lake build)",
        tiers: &[Contract],
        bound_by: "crates/building-blocks/crdt/src/fugue_tests.rs a_linear_history_is_its_ops_applied_in_turn, concurrent_inserts_at_one_spot_keep_each_run_whole_in_id_order, concurrent_deletes_of_one_character_remove_it_once",
    },
];
