use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Transactional node source grants",
        name: "HostNoopProjection rechecks descendants with the original actor",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs",
        model_ref: "MC_HostNoopProjection_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "HostSourceNoop actual changes require authority and observations validate",
        axis: Tla,
        anchor: "crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/nac/trait_impl.rs",
        model_ref: "MC_HostSourceNoop_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "ProjectorMutationOrigin actual actor and atomic publication",
        axis: Tla,
        anchor: "crates/database/db/src/document_projection.rs; crates/database/db/src/nac/trait_impl.rs",
        model_ref: "MC_ProjectorMutationOrigin_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "INV_Authority / INV_Private / INV_NoRevival / INV_Independent / INV_Reconciled / INV_RequestUnchanged",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/node_acp/source.rs; crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs",
        model_ref: "MC_SourceScopedNac_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Transactional node source grants",
        name: "remove_other / remove_self / direct_survives / owner_survives / admin_survives / denied / untrusted_noop_denied / embedded_unchanged_allowed / embedder_replay_reconciles / embedder_replay_never_widens / embedder_missing_unauthorized_refused / embedder_complete_replay_needs_no_authority / embedder_replay_ignores_node_authority / untrusted_no_replay",
        axis: Lean,
        anchor: "crates/engine/acp/src/nac/node_acp/source.rs; crates/engine/query/src/txn/source_nac.rs; crates/database/db/src/document_projection.rs; crates/entry-points/defra-node/tests/unit/source_nac.rs",
        model_ref: "SourceScopedNac (lake build)",
        tiers: &[Boundary, Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/source_nac.rs source_admission_matches_executed_lean_model",
    },
];
