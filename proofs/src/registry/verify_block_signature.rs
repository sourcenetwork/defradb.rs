use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "defra-node, db, acp (#2724)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2724: verify-signature's NAC, document ACP and block checks decide
        // at the caller's transaction view, or the read-only one the pool path
        // opens: NAC through check_node_access_in_txn, document ACP bound to
        // the view's ACP store.
        family: "Verify block signature",
        name: "INV_VerifiedAtOnePoint / INV_NoValidRejected",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/ops/block.rs NodeOps::verify_block_signature; crates/database/db/src/txn/registry/blocks.rs DbTransactionRegistry::verify_block_signature_in_txn_as; crates/database/db/src/block/verify.rs verify_block_signature_as, verify_block_signature_in_txn_as, verify_block_signature_with_txn, load_authorized_block_signature; crates/database/db/src/access/nac.rs DB::check_node_access_in_txn",
        model_ref: "MC_VerifySignature_{Green_*,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
