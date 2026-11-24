use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db, acp, vera, http, cli, ffi, defra-node";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Add DAC policy",
        name: "INV_NoPolicyWithoutPermission / INV_NoPolicyWithoutCreator / INV_NoInvalidPolicy / INV_NoPolicyWithoutAcp / INV_ReturnedIsKeptLocally / INV_NoSilentFailure / INV_GoOrder / INV_SeedHonoured",
        axis: Tla,
        anchor: "crates/database/db/src/access/dac/mod.rs DB::add_dac_policy_as, DB::add_dac_policy_with_counter_as, DB::add_dac_policy_with_nonce_as, DB::add_policy, parse, build; crates/database/db/src/access/dac/policies.rs DacPolicies::keep, DacPolicies::get; crates/engine/acp/src/policy_lookup.rs RemotePolicies::add_policy",
        model_ref: "MC_DacPolicyAdd_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
