use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A server's connections are tasks the drain waits for: a request in
        // flight when shutdown begins finishes and is answered, a connection
        // looks at closing only between requests, and the drain's deadline
        // stops what is left. A subscription stream, which has no end of its
        // own, ends with its complete event once closing began, so the
        // deadline stops no stream; it may still stop a slow or stuck
        // ordinary request.
        family: "Server drain",
        name: "INV_NoDropBeforeDeadline / INV_NoNewWorkWhenClosing / INV_DeadlineStopsNoStream / LIVE_ServerReturns: a request in flight is only stopped at the drain's deadline, no request starts once closing began, the deadline stops no subscription stream, the server returns",
        axis: Tla,
        anchor: "crates/building-blocks/net/src/server.rs serve, until_closing; crates/building-blocks/net/src/rt.rs Executor::execute; crates/database/pg-compat/src/lib.rs PgServer::run; crates/database/pg-compat/src/wire/process.rs serve; crates/foundation/edge/src/handle.rs Scope::drain; crates/surfaces/http/src/handlers/closing.rs next_unless_closing; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/surfaces/http/src/handlers/events.rs events_sse",
        model_ref: "MC_ServerDrain_{Green,Red_Service,Red_CheckInside,Red_NoCheck,Red_NoDeadline,Red_StreamIgnoresClosing}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/server.rs a_closing_server_finishes_the_requests_in_flight_then_returns; crates/database/pg-compat/tests/wire.rs a_query_in_flight_when_the_edge_starts_closing_completes_and_the_connection_closes, an_idle_connection_closes_at_once_when_the_edge_starts_closing; crates/surfaces/http/tests/sse_ends_when_closing.rs a_graphql_subscription_ends_when_its_edge_closes, the_event_stream_ends_when_its_edge_closes; crates/entry-points/defra-node/tests/subscription_shutdown.rs shutdown_ends_an_open_graphql_subscription_without_waiting_out_the_drain",
    },
];
