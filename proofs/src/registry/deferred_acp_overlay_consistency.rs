use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "query";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Deferred-ACP overlay consistency",
        name: "INV_FailClosedActive — txn-local ACP projection gates as committed state would",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/context.rs AcpRegistrationProjection::owed_creator, check_doc_access_with_projection, is_doc_registered_with_projection, is_creator; crates/database/db/src/txn/mod.rs DbTxn::acp_registration_projection; crates/engine/acp/src/pending.rs PendingRegistrations::record, PendingRegistrations::get; crates/database/db/src/write/autocommit/helpers.rs register_created_doc; crates/database/db/src/collection/acp.rs release_purged_doc_objects",
        model_ref: "MC_DeferredAcp_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/deferred_acp.rs deferred_acp_txn_local_gating",
    },
];
