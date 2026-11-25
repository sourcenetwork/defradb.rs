use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "events, http, query, ffi";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "SSE loss and error delivery",
        name: "INV_DetectedLossEnds / INV_ErrorForwarded / LossEndsStream",
        axis: Tla,
        anchor: "crates/foundation/events/src/channel_bus.rs ChannelBus::publish_raw; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/surfaces/http/tests/graphql_sse_recovery.rs",
        model_ref: "MC_SseDelivery_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/graphql_sse_recovery.rs overflow_ends_the_graphql_stream_before_a_queued_result, query_error_is_delivered_even_without_rows",
    },
    Property {
        family: "SSE loss and error delivery",
        name: "loss_requires_recovery / loss_aware_closes / raw_stays_open / errors_are_forwarded / empty_is_skipped / denial_has_code / other_has_no_denial",
        axis: Lean,
        anchor: "crates/foundation/events/src/channel_bus.rs ChannelBus::publish_raw; crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/engine/query/src/executor.rs QueryResponseError::from_query_error",
        model_ref: "SseDelivery (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/foundation/events/tests/unit/channel_bus.rs overflow_wakes_reader_that_drained_before_loss_was_recorded, ordinary_raw_subscription_remains_open_after_overflow; crates/surfaces/http/tests/graphql_sse_recovery.rs typed_permission_refusal_keeps_its_code_in_the_graphql_stream",
    },
    Property {
        // A subscription is subscribed first and checked once through the
        // plain path after, raced with closing: a bad filter is the stream's
        // only result, a check that failed for a reason that passes keeps
        // the stream (and the update that finds it bad reports it), and no
        // update committed after the registration is lost.
        family: "SSE loss and error delivery",
        name: "INV_BadNeverStreams / INV_BadIsReported / INV_GoodNeverRefused / INV_NoLostUpdate / LIVE_ClosingEnds / LIVE_BadUpdateReported: a bad filter never streams and is reported, a good one is never refused even when its check met a failure that passes, an update committed while the subscription is registered reaches a valid subscription, closing ends every stream, a bad request kept past a failure that passes is reported by the update that finds it",
        axis: Tla,
        anchor: "crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/entry-points/ffi/src/query/exec.rs subscribe; crates/engine/query/src/subscription.rs open_failure, update_refused; crates/engine/query/src/error.rs QueryError::is_invalid_request; tools/integration-test/src/actions/query/subscription.rs SubscriptionRequest::open",
        model_ref: "MC_SubscriptionOpenCheck_{Green,Red_SkipCheck,Red_CheckThenSubscribe,Red_CheckIgnoresClosing,Red_TransientRefuses,Red_UpdateDropsInvalid}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/graphql_sse_recovery.rs a_subscription_the_plain_path_refuses_sends_its_error_then_completes, a_transient_failure_at_open_keeps_the_stream; crates/engine/query/tests/filter_shape_checks.rs a_failed_lookup_does_not_make_the_request_invalid, a_bad_cursor_argument_makes_the_request_invalid; crates/engine/query/src/subscription.rs only_an_invalid_request_refuses_an_update; crates/surfaces/http/tests/sse_ends_when_closing.rs a_graphql_subscription_ends_when_its_edge_closes; tools/integration-test/tests/testo/subscription/with_filter_unknown.rs subscription_with_an_unknown_filter_field_is_refused_on_open, subscription_with_a_valid_filter_variable_streams",
    },
    Property {
        family: "SSE loss and error delivery",
        name: "bad_open_reports_then_completes / transient_open_keeps_stream / closing_open_completes / only_invalid_open_is_refused / ffi_invalid_update_reports_then_ends / ffi_only_invalid_update_ends: an invalid open sends its error and completes, a check that failed for a reason that passes keeps the stream, closing completes it, only an invalid open is refused, and an FFI update that finds the request invalid sends it and ends the subscription, which nothing else ends",
        axis: Lean,
        anchor: "crates/surfaces/http/src/handlers/graphql/query.rs graphql_sse; crates/engine/query/src/subscription.rs open_failure, update_refused; crates/entry-points/ffi/src/query/exec.rs subscribe",
        model_ref: "SseDelivery (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/surfaces/http/tests/graphql_sse_recovery.rs a_subscription_the_plain_path_refuses_sends_its_error_then_completes, a_transient_failure_at_open_keeps_the_stream; crates/engine/query/src/subscription.rs only_an_invalid_request_refuses_an_update",
    },
];
