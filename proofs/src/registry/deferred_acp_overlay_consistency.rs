use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/context.rs AcpRegistrationProjection::owed_creator, check_doc_access_with_projection, is_doc_registered_with_projection, is_creator; crates/database/db/src/txn/mod.rs DbTxn::acp_registration_projection; crates/engine/acp/src/pending.rs PendingRegistrations::record, PendingRegistrations::get, PendingRegistrations::clear; crates/database/db/src/write/autocommit/helpers.rs register_created_doc, unregister_deleted_doc",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/deferred_acp.rs deferred_acp_txn_local_gating",
    },
];
