use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "net";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Connect fallback race",
        name: "INV_OneSocket / INV_FallbackWaits / INV_NeverIdle / INV_ErrorOnlyWhenAllFail / INV_LastError / INV_WithinLimit / LIVE_Ends: one socket comes back, the other family starts after the delay or a failure, an error only once every family failed",
        axis: Tla,
        anchor: "crates/building-blocks/net/src/connect.rs first_connected, Connector::socket, FALLBACK_DELAY",
        model_ref: "MC_ConnectFallback_{Green,Green_NoTimeout,Green_Single,Red_NoDelay,Red_WaitAfterFailure,Red_FirstErrorWins,Red_KeepFirstError,Red_KeepLoser,Red_TimeoutMissesRace,Red_TimeoutMissesRaceHangs}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/connect.rs the_other_family_connects_when_the_first_is_silent_past_the_delay, the_other_family_is_tried_when_the_first_refuses, when_both_families_refuse_the_last_refusal_comes_back",
    },
];
