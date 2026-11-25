use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs delegation window",
        name: "issued_is_well_formed / issued_is_what_veras_builder_writes / request_holds_iff / written_at_provider_time_holds / ahead_within_allowance_holds / ahead_beyond_allowance_refused / behind_by_the_lifetime_refused / longer_than_the_longest_lifetime_refused_ahead / plain_ahead_within_allowance_holds / without_allowance_one_second_ahead_refused / reported_lead_is_a_lower_bound / unset_start_is_reported_exactly / set_back_start_is_reported_less_the_allowance / expired_claims_nothing_about_the_clock / outlasting_reaches_too_far / rewritten_at_reported_time_holds",
        axis: Lean,
        anchor: "crates/engine/vera-delegation/src/bearer/request.rs issue_request_at, MAX_REQUEST_LIFETIME; crates/engine/vera-delegation/src/bearer.rs not_before; crates/engine/vera/src/vera_rs/bearer.rs authorize_at; crates/engine/vera/src/vera_rs/bearer/window.rs within_window; crates/engine/acp/src/delegation_window.rs CLOCK_SKEW_ALLOWANCE, WindowRefusal",
        model_ref: "VeraDelegationWindow (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
