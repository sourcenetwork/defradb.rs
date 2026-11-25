use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-core, db, http, p2p, bitswap";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "CID content-addressing determinism + Block canonicalization",
        name: "cid_injective_mod_hash — same normal-form content yields same CID",
        axis: Lean,
        anchor: "crates/foundation/defra-core/src/block.rs Block::generate_cid",
        model_ref: "Cid (lake build)",
        tiers: &[Behavioral, Contract],
        bound_by: "proofs/tests/behavioral/cid.rs cid_determinism_same_content_same_docid",
    },
    Property {
        // proofs/lean/Cid/SignedGenesis.lean: node_commits_precomputed,
        // client_signed_commits_precomputed and peer_commits_creators_id with
        // their RED theorems, node_recompute_after_schema_change_moves_id (the
        // node promise holds only on the prepared schema), the rekey and import
        // exceptions, the signature-reach and determinism theorems,
        // collision_refused, and the RED resign_after_encode_breaks_doc_id.
        //
        // Boundary: no `proofs/tests/behavioral/` scenario drives the release
        // binary through this property; the crate unit tests, the testo
        // scenarios and the Lean theorems are what keep it honest.
        family: "CID content-addressing determinism + Block canonicalization",
        name: "node_commits_precomputed, client_signed_commits_precomputed, peer_commits_creators_id — a signed create commits the DocID its creator computed",
        axis: Lean,
        anchor: "crates/database/db/src/block/builder/compute.rs compute_document_blocks; crates/database/db/src/block/builder/mod.rs derive_doc_id; crates/database/db/src/write/prepare.rs PreparedCreateBlocks; crates/database/db/src/write/create.rs compute; crates/database/db/src/merge/merge_handler/doc_identity.rs resolve_composite_doc_id; crates/engine/blockstore/src/verify.rs verify_block_cid; crates/database/p2p/src/sync/manager/process/pushlog.rs process_block_inner; crates/building-blocks/bitswap/src/message/mod.rs verify_hash; crates/database/db/src/definition/migration/shortid.rs resolve_legacy_identities; crates/entry-points/defra-node/src/lib.rs execute_verified_graph_in_txn; crates/surfaces/http/src/client_signed_create.rs conversation_on_socket; crates/database/db/src/write/autocommit/helpers.rs register_created_doc; crates/engine/query/src/documents/response.rs refused; crates/surfaces/http/src/handlers/documents.rs refused; crates/entry-points/cli/src/commands/client/http_client/mod.rs extract_error",
        model_ref: "Cid (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/defra-node/tests/unit/verified_create.rs verified_user_create_shares_node_transaction_with_agent_effects, verified_create_on_an_identity_only_node_is_signed_by_its_caller; crates/database/db/tests/block/builder_tests.rs test_build_blocks_derives_doc_id_from_genesis_cid; crates/database/db/tests/block/client_create_graph.rs trusted_create_graph_matches_model_and_rejects_unapproved_waves; crates/engine/blockstore/src/verify.rs tests::tampered_data_fails, tests::unsupported_algorithm_fails; crates/database/db/tests/write/collision_code.rs a_second_create_is_refused_as_existing_with_its_code, a_create_of_a_deleted_document_is_refused_as_deleted_with_its_code; tools/integration-test/tests/testo/signature/peer.rs doc_signature_with_peers_an_different_key_types_should_sync; tools/integration-test/tests/testo/signature/doc_id_determinism.rs a_peer_shows_the_creators_doc_id, the_same_key_and_content_give_the_same_doc_id_ed25519, the_same_key_and_content_give_the_same_doc_id_secp256k1, a_different_key_gives_a_different_doc_id; tools/integration-test/tests/testo/signature/client_signed.rs a_client_signed_create_commits_the_precomputed_id, a_client_signed_create_is_queryable_under_the_precomputed_id; tools/integration-test/tests/testo/txn/signed_doc_id.rs a_node_signed_create_in_a_txn_commits_the_id_it_returned_ed25519, an_autocommit_node_signed_create_stores_the_id_it_returned; tools/integration-test/tests/testo/mutation/add/signed_duplicate.rs a_node_signed_duplicate_create_is_refused_with_its_code, a_client_signed_duplicate_create_is_refused; tools/integration-test/tests/testo/backup/simple/import_alias.rs an_imported_document_gets_a_new_id_and_its_file_id_resolves_to_it",
    },
];
