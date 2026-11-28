use super::{Axis::*, Property, Tier::*};

pub(super) const CRATES: &str = "acp, db, defra-node, cli";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Refused create cleanup",
        name: "ReservationsAgree / ReplacementSafe / EventuallyReleased",
        axis: Tla,
        anchor: "crates/engine/acp/src/external/registration.rs ExternalDocumentAcp::note_failure; crates/database/db/src/access/refused_create.rs DB::purge_refused_create, purge_refused_creates",
        model_ref: "MC_RefusedCreateCleanup_{Green,RedSplit,RedReplay,RedOwner}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Refused create cleanup",
        name: "release_own / release_twice / release_other",
        axis: Lean,
        anchor: "crates/database/db/src/access/refused_create.rs DB::purge_refused_create",
        model_ref: "RefusedCreateCleanup.lean",
        tiers: &[Boundary],
        bound_by: "",
    },
];
