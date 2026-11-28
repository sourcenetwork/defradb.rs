use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "client, db, query, http, ffi, wasm, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A transaction ID travels in the request context. Only the caller
        // that opened the transaction runs in it, a read-only transaction
        // refuses writes, a finished transaction runs nothing, a discard
        // leaves nothing in the store, and a commit that clashes with a
        // newer commit fails.
        family: "Explicit transactions in the request context",
        name: "INV_OnlyOwnerRuns / INV_ReadOnlyNeverWrites / INV_FinishedNeverRuns / INV_DiscardLeavesNothing / INV_NoLostUpdate",
        axis: Tla,
        anchor: "crates/database/db/src/txn/registry/lifecycle.rs begin_as, run_as, commit_as, discard_as; crates/database/db/src/txn/owner/mod.rs Owners::run, Owners::finish; crates/entry-points/defra-node/src/node_core/transaction.rs NodeCore::begin, NodeCore::commit, NodeCore::discard; crates/entry-points/defra-node/src/scope/txn_node.rs TxnNode::commit, TxnNode::discard; crates/engine/query/src/runner/turn.rs read-only rejection; tools/integration-test/tests/testo/txn/txn_context.rs",
        model_ref: "MC_TxnContext_{Green,Red_NoOwnerCheck,Red_ReadOnlyWrites,Red_RunAfterEnd,Red_DiscardLeaks,Red_NoConflictCheck}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // An explicit transaction's ID is `sequence * workers + worker`. The
        // owner is the ID modulo the worker count and the sequence is the
        // quotient, so the round trip is exact for every worker below the
        // count, the owner is always a real worker, and two different
        // (sequence, worker) pairs never share an ID. The start token packs
        // above the route and unpacks exactly, and the packed ID stays below
        // 2^53 so a JSON number reader keeps it. A worker past its routes
        // rolls to the next nonzero token, and its route still fits and still
        // names it.
        family: "Explicit transactions in the request context",
        name: "TxnId.owner_encode / sequence_encode / owner_lt / encode_injective / decode_encode / route_pack / token_pack / pack_json_safe / rolled_route_lt / rolled_owner / rolled_injective / advance_pos / advance_lt",
        axis: Lean,
        anchor: "crates/database/db/src/txn/owner/id.rs TxnId::new, TxnId::advance, TxnId::owner, TxnId::start_token, TxnId::parse; crates/database/db/src/txn/owner/tests.rs",
        model_ref: "TxnId (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
