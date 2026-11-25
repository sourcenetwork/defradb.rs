use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "edge, net (#2546)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Work that continues a request already admitted (an outbound
        // connection's driver) is admitted after the gate closes while
        // admitted work is in flight; the drain seals before its last look, so
        // it never says drained while a continuation still runs.
        family: "Edge gate continuation",
        name: "INV_NothingAfterDrain / INV_InFlightContinues / INV_NoContinuationWhenIdle: nothing holds a permit once drained, a request in flight gets its continuation, and none starts from an idle closed gate",
        axis: Tla,
        anchor: "crates/foundation/edge/src/gate.rs Gate::admit, Gate::admit_continuation, Gate::drained; crates/foundation/edge/src/handle.rs Edge::spawn_continuation, drain_gate; crates/building-blocks/net/src/rt.rs ClientExecutor::execute",
        model_ref: "MC_EdgeGateContinuation_{Green,Red_PlainSpawn,Red_NoSeal,Red_NoCas}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/foundation/edge/tests/continuation.rs a_continuation_is_admitted_after_close_while_admitted_work_runs, a_continuation_is_refused_after_close_when_nothing_is_in_flight, the_drain_waits_for_a_running_continuation, a_continuation_is_refused_after_the_drain_completed_even_with_plain_spawns_racing; crates/building-blocks/net/tests/server.rs a_closing_client_finishes_the_request_in_flight_and_frees_its_idle_connections, a_request_in_flight_at_close_opens_a_fresh_connection_and_finishes",
    },
];
