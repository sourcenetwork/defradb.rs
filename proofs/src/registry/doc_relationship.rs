use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, defra-node, cli (#2321)";

pub(super) const PROPERTIES: &[Property] = &[Property {
    family: "Document actor relationship checks",
    name: "INV_NoChangeUnlessAllChecksPass / INV_GoOrder / INV_EventOnlyIfChanged / INV_SameAnswer",
    axis: Tla,
    anchor: "crates/database/db/src/access/dac/client/mod.rs DocumentRelationships::add_as, DocumentRelationships::delete_as, DocumentRelationships::change_as, DocumentRelationships::apply; crates/database/db/src/access/dac/client/checks.rs DocumentRelationships::requestor, DocumentRelationships::requestor_in_txn, DocumentRelationships::collection_policy, DocumentRelationships::relationship_target, check_txn_node_permission; crates/database/db/src/access/dac/client/in_txn.rs DocumentRelationships::autocommit, DocumentRelationships::in_txn, AutocommitTxn; crates/database/db/src/access/dac/client/settle.rs publish_document_update; crates/entry-points/defra-node/src/acp_ops.rs DbAcpOps; crates/entry-points/cli/src/doc_acp_adapter.rs DocumentAcpAdapter; crates/surfaces/http/src/handlers/acp.rs add_doc_relationship, add_doc_relationships, remove_doc_relationship",
    model_ref: "MC_DocRelationship_{Green,Red_*}.cfg",
    tiers: &[Boundary],
    bound_by: "crates/entry-points/defra-node/tests/unit/transactional_relationship.rs a_transaction_checks_get_collection_as_the_autocommit_route_does; crates/entry-points/cli/tests/doc_acp_server.rs checks_and_transactional_grants_match_across_the_started_server; tools/integration-test/tests/testo/acp/dac/relationship/doc_actor/checks.rs a_transaction_uses_gos_check_order_before_the_owner_relation, a_transaction_wraps_a_policy_refusal_as_autocommit_does",
}];
