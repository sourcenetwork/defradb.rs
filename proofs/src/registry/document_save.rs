use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Document save",
        name: "INV_NoFork / INV_DeletedStaysDeleted / INV_RefusedOnlyDeleted",
        axis: Tla,
        anchor: "crates/engine/query/src/documents/mod.rs save; crates/entry-points/defra-node/src/ops/document.rs Node::save_doc; tools/integration-test/tests/testo/mutation/update/with_delete.rs update_save_deleted_doc_does_nothing",
        model_ref: "MC_DocumentSave_{Green,Red_NoDeletedCheck,Red_SplitTxn}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Document save",
        name: "decide_refuses_deleted / refuse_only_deleted / save_never_forks / save_under_given_id",
        axis: Lean,
        anchor: "crates/engine/query/src/documents/mod.rs save; crates/engine/query/src/plan/mutation/upsert.rs UpsertNode::next",
        model_ref: "DocumentSave (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
