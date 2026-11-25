use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "NAC lifecycle privilege-escalation",
        name: "INV_NoPrivEsc — no escalation across enable/disable/restart",
        axis: Tla,
        anchor: "crates/database/db (nac) lifecycle",
        model_ref: "MC_Nac_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac_lifecycle.rs nac_lifecycle_no_escalation",
    },
    Property {
        // Enabling sets NAC up only the first time: neither starting the node
        // with the enable option nor an enable call turns a temporarily
        // disabled NAC back on, so only an authorized re-enable does, as in Go.
        family: "NAC lifecycle privilege-escalation",
        name: "INV_ReEnableNeedsPersistedAdmin through enable and startup: a disabled NAC stays disabled until an authorized re-enable",
        axis: Tla,
        anchor: "crates/engine/acp/src/nac/node_acp/lifecycle.rs NodeACP::enable; crates/database/db/src/nac/mod.rs NacManager::initialize",
        model_ref: "MC_Nac_{Green,Red_EnableReEnables,Red_EnableAddsOwner,Red_StartReEnables}.cfg",
        tiers: &[Contract],
        bound_by: "crates/engine/acp/tests/nac_tests.rs enabling_a_disabled_nac_is_refused; crates/database/db/tests/nac/tests.rs a_disabled_nac_stays_disabled_when_restarted_with_the_enable_option, enabling_a_disabled_nac_is_refused; crates/entry-points/defra-node/tests/unit/http_acp.rs a_stranger_cannot_enable_a_disabled_nac_over_http",
    },
];
