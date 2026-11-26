use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, db-merge, query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Committed document update discovery",
        name: "INV_Atomic / INV_Once / INV_OnlyChanges / INV_Authorized / INV_DeletedHidden / INV_UnregisteredAfterDelete / INV_CursorBound / INV_NoSkippedVisible — a block that changed its document is journaled once in its transaction, one that lost to the stored value is not; a page of up to Limit rows is decided at its own snapshot, or confirmed after a provider outside its transaction answered, which loses a registration only after this node's delete; a deleted document's rows are shown to nobody. Not modeled: the reader's cursor-beyond-head and missing-row errors. Not prevented: a registration a peer took from a shared provider (Red_Unordered)",
        axis: Tla,
        anchor: "crates/database/db/src/event/updates.rs record/read/rows/drain; crates/database/db/src/event/arrivals.rs read; crates/database/db/src/event/gate.rs Gate::decide, Gate::confirm; crates/database/db/src/txn/mod.rs DbTxn::later; crates/database/db/src/docid/map.rs peek_doc_short_id; crates/database/db/src/write/update.rs write_blocks; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document; crates/database/db/src/merge/merge_handler/field_state.rs merge_field_state; crates/engine/query/src/runner/arrivals.rs execute_arrivals_query",
        model_ref: "MC_DocumentUpdateJournal_{Green,Green_Outside}.cfg / MC_DocumentUpdateJournal_Red_{Detached,Duplicate,Superseded,Authorization,Deleted,Stale,Unordered,Skip}.cfg",
        tiers: &[Behavioral],
        bound_by: "crates/database/db/tests/read/update_transactions.rs commit_order_rollback_and_replayed_updates, updates_and_replay_checkpoint_survive_reopen, a_scan_returns_the_rows_of_its_collection_and_range_only, a_scan_is_closed_when_a_row_is_missing; crates/database/db/tests/read/arrivals.rs graphql_updates_page_each_committed_version_of_one_document, hidden_updates_advance_page_without_disclosing_identity_or_cid; crates/database/db/tests/read/update_visibility.rs a_deleted_private_documents_updates_are_shown_to_nobody, a_purged_private_documents_updates_are_shown_to_nobody, one_page_decides_each_document_once, a_page_in_a_transaction_decides_at_that_transactions_snapshot, a_page_in_a_writing_transaction_does_not_fail_it_when_its_document_is_deleted, a_page_in_a_writing_transaction_conflicts_when_its_documents_object_is_purged, a_delete_during_the_page_hides_the_row_from_a_provider_outside_its_transaction, a_transaction_fetcher_pages_for_a_provider_outside_its_transaction; crates/database/db/tests/read/update_visibility/transaction_fetcher.rs a_delete_during_the_page_hides_the_row, a_page_in_a_writing_transaction_reads_its_later_snapshot_too, a_transaction_no_database_began_says_why_it_cannot_confirm; crates/database/db/tests/read/update_page.rs a_page_reads_its_rows_in_one_scan; crates/database/db/tests/docid/map.rs a_peek_resolves_what_a_get_resolves; crates/database/db/tests/merge/merge_handler_tests.rs peer_update_is_discovered_once_by_committed_cursor; crates/database/db/tests/merge/update_journal.rs a_merge_that_loses_to_the_stored_value_adds_no_row, a_block_merged_again_by_another_handler_adds_no_second_row, a_block_whose_field_was_merged_on_its_own_first_still_gets_its_row; tools/integration-test/tests/testo/subscription/delivery/arrivals.rs hidden_arrivals_are_passed_and_not_shown",
    },
    Property {
        family: "Committed document update discovery",
        name: "append_idempotent / different_blocks_are_distinct / superseded_adds_no_row / unauthorized_not_visible / deleted_not_visible / confirmed_rows_are_allowed / page_within_limit / pages_join / once_key_injective",
        axis: Lean,
        anchor: "crates/database/db/src/event/updates.rs record/rows/once_key; crates/database/db/src/event/arrivals.rs read; crates/database/db/src/event/gate.rs Gate::decide, Gate::confirm; crates/database/db/src/merge/merge_handler/composite_persist.rs persist_merged_document",
        model_ref: "DocumentUpdateJournal (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/read/update_transactions.rs identities_are_unambiguous_and_payloads_round_trip, commit_order_rollback_and_replayed_updates, a_scan_returns_the_rows_of_its_collection_and_range_only; crates/database/db/tests/read/arrivals.rs graphql_updates_page_each_committed_version_of_one_document, hidden_updates_advance_page_without_disclosing_identity_or_cid; crates/database/db/tests/read/update_visibility.rs a_deleted_private_documents_updates_are_shown_to_nobody, a_delete_during_the_page_hides_the_row_from_a_provider_outside_its_transaction; crates/database/db/tests/merge/update_journal.rs a_merge_that_loses_to_the_stored_value_adds_no_row",
    },
];
