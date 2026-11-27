use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, acp, http, cli, ffi, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Node access control operations",
        name: "INV_NoStatusWithoutPermission / INV_NoDisableWithoutPermission / INV_NoReEnableWithoutStoredPermission / INV_NoGrantUnlessAllChecksPass / INV_NoChangeUnlessConfigured / INV_RestartKeepsStatus / INV_NoSilentNoOp / INV_GoOrder / INV_RefusedWhenGoRefuses",
        axis: Tla,
        anchor: "crates/database/db/src/access/nac_ops/mod.rs DB::nac_status_as, DB::disable_nac_as, DB::re_enable_nac_as, DB::add_nac_actor_relationship_as, DB::delete_nac_actor_relationship_as, DB::change_nac_relationship; crates/database/db/src/nac/toggle.rs NacManager::decide_toggle, NacManager::require; crates/database/db/src/nac/write.rs NacManager::decide_relationship, NacTarget::parse; crates/database/db/src/nac/mod.rs NacManager::initialize; crates/engine/acp/src/nac/node_acp/operations.rs NodeACP::authorize_change",
        model_ref: "MC_NodeAcpOps_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/legacy/nac/core_operations.rs nac_core_operations_gate_test; crates/database/db/tests/nac/tests.rs test_nac_manager_disable_reenable, test_nac_manager_permission_check, an_empty_target_is_refused_and_never_reaches_the_wildcard",
    },
    Property {
        // A NAC relationship change refuses an empty target with Go's
        // ErrRelationshipTargetActorEmpty, after the identity and before the
        // relation, on add and delete alike, so it never reaches `*`.
        family: "Node access control operations",
        name: "empty_target_never_changes / empty_target_is_refused_as_go / identity_is_checked_before_the_target / delete_of_a_non_did_finds_nothing",
        axis: Lean,
        anchor: "crates/database/db/src/nac/write.rs NacManager::decide_relationship, NacTarget::parse; crates/database/db/src/access/nac_ops/mod.rs DB::change_nac_relationship",
        model_ref: "NacRelationship (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node access control operations",
        name: "INV_RaceNoChangeWithoutPermission / INV_RaceOneWinner / INV_RaceLoserHearsAlready",
        axis: Tla,
        anchor: "crates/database/db/src/nac/toggle.rs NacManager::toggle_as, NacManager::decide_toggle; crates/database/db/src/nac/write.rs NacManager::in_write, NacManager::decide_relationship; crates/engine/acp/src/zanzibar/store/persistent.rs PersistentZanzibarStore::transact; crates/database/db/tests/access/nac_toggle_race.rs",
        model_ref: "MC_NodeAcpOps_{Race_Green,Red_SplitScope,Red_OutsideGuard,Red_NoRetry}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/nac_toggle_race.rs of_two_disables_one_wins_and_the_other_is_already_disabled",
    },
    Property {
        family: "Node access control operations",
        name: "INV_RelNoChangeWhileOff / INV_RelOffHearsGo",
        axis: Tla,
        anchor: "crates/database/db/src/nac/write.rs NacManager::in_write, NacManager::decide_relationship; crates/database/db/src/access/nac_ops/mod.rs DB::change_nac_relationship; crates/database/db/tests/access/nac_toggle_race.rs",
        model_ref: "MC_NodeAcpOps_{Rel_Green,Red_RelOutsideGuard}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/access/nac_toggle_race.rs a_disable_that_commits_first_refuses_the_relationship_with_gos_error",
    },
    Property {
        family: "Node access control operations",
        name: "disable_turns_off / re_enable_turns_on / re_enable_undoes_disable / toggles_keep_grants / writes_only_when_allowed / writes_only_when_configured / disable_writes_only_from_on / re_enable_writes_only_from_off / re_enable_checks_the_caller_first / rerun_after_a_win_hears_already / enable_writes_only_when_not_configured / enable_refuses_a_disabled_nac / enable_leaves_a_disabled_store / startup_keeps_a_disabled_nac_off",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/node_acp/mod.rs NodeACP::load; crates/engine/acp/src/nac/node_acp/lifecycle.rs NodeACP::disable, NodeACP::re_enable, NodeACP::enable; crates/database/db/src/nac/toggle.rs NacManager::decide_toggle; crates/database/db/src/nac/mod.rs NacManager::initialize",
        model_ref: "NacToggle (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A document write or a truncate checks `get-collection` and then
        // its own permission, in Go's order, inside its transaction, and a
        // call that names no identity acts as no one, never as the node.
        // Over HTTP the edge checks first, in the same order, from a
        // snapshot of the grants swapped in before any NAC write returns.
        family: "Node access control operations",
        name: "INV_NoEffectWithoutPermission / INV_RefusalNamesGoFirst / INV_EdgeSeesReturnedRevoke",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/permission.rs NodePermission::go_order; crates/database/db/src/access/nac.rs check_node_access_as, check_node_access_in_txn; crates/database/db/src/write/autocommit/batch.rs check_attempt_node_access; crates/database/db/src/txn/registry/indexes.rs check_access_in; crates/database/db/src/collection/encrypted_index/client/mod.rs DbTransactionRegistry::check_encrypted_index_access; crates/engine/query/src/runner/turn.rs lent, lent_node_permission; crates/entry-points/defra-node/src/ops/request.rs admitted_caller, txn_owner; crates/surfaces/http/src/auth_middleware.rs auth_middleware; crates/surfaces/http/src/route_permissions.rs reads_collection_first, route_permission; crates/surfaces/http/src/nac_guard.rs require_permissions; crates/surfaces/http/src/nac_adapter.rs NacAdapter::check_permissions; crates/database/db/src/nac/mod.rs NacManager::check_permissions; crates/database/db/src/nac/edge.rs refresh_edge, refresh_edge_on_commit, refresh_guard, RefreshGuard, reload_after_commit, EdgeGrants::publish, EdgeSnapshot::check, check_all, decide; crates/engine/acp/src/nac/node_acp/snapshot.rs copy_into; crates/database/db/src/nac/write.rs in_write; crates/database/db/src/nac/mod.rs initialize, enable, purge; crates/entry-points/defra-node/src/node_builder.rs NodeBuilder::build",
        model_ref: "MC_NacOrder_{Green,Red_SkipGet,Red_OwnFirst,Red_AnonAsNode,Red_OutsideTxn,Red_EdgeOwnOnly,Red_StaleEdge,Red_NoWithdraw}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/entry-points/cli/src/nac_adapter_tests.rs a_returned_revocation_refuses_the_next_edge_check, the_snapshot_decides_every_permission_as_the_engine_does; crates/database/db/tests/nac/dropped_write.rs a_revoke_dropped_after_its_commit_still_checks_the_edge",
    },
    Property {
        // A database transaction that commits a NAC tuple write (through a
        // projector or a source grant) rebuilds the edge snapshot at commit;
        // one that writes none registers nothing and rebuilds nothing. A
        // savepoint rolled back drops the registrations made after it.
        family: "Node access control operations",
        name: "INV_RebuiltAfterNacWrite / INV_NoRebuildWithoutNacWrite",
        axis: Tla,
        anchor: "crates/engine/acp/src/zanzibar/store/transaction.rs written; crates/engine/datastore/src/multistore.rs node_access_written, NodeAccessWrites; crates/database/db/src/nac/edge.rs EdgeRefreshOwed, refresh_edge_on_commit; crates/database/db/src/database/mod.rs new_db_txn; crates/engine/storage/src/corekv/callbacks.rs TxnCallbacks::mark, TxnCallbacks::truncate",
        model_ref: "MC_NacEdgeRefreshOnWrite_{Green,Red_EveryWrite,Red_KeepOnRollback}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/acp/tests/node_access_writes.rs only_node_policy_writes_owe_the_edge_refresh; crates/entry-points/defra-node/tests/unit/document_projector.rs raw_document_mutations_stage_sources_in_the_same_transaction",
    },
    Property {
        family: "Node access control operations",
        name: "goOrder_collection_write / goOrder_alone / firstMissing_none_iff / firstMissing_some / allowed_needs_both / refused_get_collection_first / no_bypass_when_off / no_bypass_anonymous / no_bypass_when_denied / bypass_iff_held",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/permission.rs NodePermission::go_order; crates/database/db/src/access/nac.rs check_node_access_as, check_node_access_in_txn; crates/engine/query/src/runner/turn.rs lent, lent_node_permission; crates/database/db/src/read/lensed/fetcher/mod.rs dac_bypass; crates/foundation/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied; crates/surfaces/p2p-adapter/src/libp2p.rs P2PAdapter::check_nac; crates/surfaces/p2p-adapter/src/iroh.rs IrohP2PAdapter::check_nac",
        model_ref: "NacOrder (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // A read whose bypass the engine resolves bypasses document access
        // control exactly when node access control is on and grants the
        // caller `bypass-dac`, decided at the read's own snapshot, and its
        // caller did not deny it the bypass. A bypass the caller set on the
        // request (`RequestContext::with_dac_bypass`) is honoured as set,
        // without the engine consulting node access control.
        family: "Node access control operations",
        name: "INV_BypassOnlyWhenGranted / INV_BypassWhenGranted / INV_DeniedNeverBypasses",
        axis: Tla,
        anchor: "crates/engine/query/src/runner/turn.rs lent; crates/database/db/src/read/lensed/fetcher/mod.rs dac_bypass; crates/database/db/src/collection/acp.rs check_doc_permission_as; crates/foundation/defra-core/src/request_context.rs RequestContext::with_dac_bypass_denied, RequestContext::with_dac_bypass",
        model_ref: "MC_NacDacBypass_{Green,Red_NacOff,Red_Never,Red_Live,Red_IgnoreDenial}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
