use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "http, db-nac, pg-compat";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Management-channel auth (NAC gate)",
        name: "INV_OnlyAuthorizedManages — management ops require a valid scoped token",
        axis: Tla,
        anchor: "crates/surfaces/http manage channel; crates/database/db (nac)",
        model_ref: "MC_Auth_Green.cfg",
        tiers: &[Behavioral],
        bound_by: "proofs/tests/behavioral/nac.rs nac_management_requires_admin",
    },
];
