use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "net";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "HTTP client redirects and body deadline",
        name: "credentials_stay_with_their_origin / at_most_limit_plus_one_exchanges / endless_redirects_fail / same_origin_redirect_keeps_credentials / cross_origin_redirect_strips_credentials: a credential from the request or the defaults reaches only the request's origin, a proxy credential only its own hop, within eleven exchanges",
        axis: Lean,
        anchor: "crates/building-blocks/net/src/http/client.rs Client::execute, Client::follow, Client::exchange, redirect, REDIRECT_LIMIT",
        model_ref: "HttpClient.Redirect (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/client.rs redirects_are_followed_and_change_the_method_as_the_status_says, credentials_follow_a_redirect_only_to_the_same_origin",
    },
    Property {
        family: "HTTP client redirects and body deadline",
        name: "ends_after_timeout / at_most_one_timeout / deadline_wins_over_a_ready_piece / never_waits_past_the_deadline / no_timeout_before_the_deadline / no_limit_no_timeout: past the limit a body stream times out once, even with a piece ready, then gives its end and nothing more",
        axis: Lean,
        anchor: "crates/building-blocks/net/src/http/response.rs BytesStream::poll_next, Response::bytes_stream",
        model_ref: "HttpClient.BodyDeadline (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/building-blocks/net/tests/body_deadline.rs a_stalled_body_stream_gives_one_timeout_then_ends, every_chunk_read_past_the_limit_times_out_at_once, a_piece_ready_past_the_limit_gives_the_timeout",
    },
];
