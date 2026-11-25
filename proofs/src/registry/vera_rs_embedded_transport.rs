use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "vera-embedded";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "vera.rs embedded transport",
        name: "INV_Recovers: a queued copy can authenticate after the prefetched head fails transiently",
        axis: Tla,
        anchor: "crates/engine/vera-embedded/src/acp.rs prepend_head",
        model_ref: "MC_HeaderRetry_{Green,Red_Drop}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/src/acp/tests.rs queued_headers_survive_a_failed_head_authentication",
    },
    Property {
        family: "vera.rs embedded transport",
        name: "HeaderRetry.failed_head_keeps_queue / retry_remains / old_filter_loses_copy",
        axis: Lean,
        anchor: "crates/engine/vera-embedded/src/acp.rs prepend_head",
        model_ref: "HeaderRetry (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/src/acp/tests.rs queued_headers_survive_a_failed_head_authentication",
    },
    Property {
        family: "vera.rs embedded transport",
        name: "within_iff / network_accepted_is_accepted / longer_is_refused / answer_in_time_is_kept / silent_node_ends_the_wait / late_answer_is_not_waited_for / node_error_keeps_its_code / busy_node_is_asked_again / slow_node_is_asked_again / stopped_node_is_not_asked_again / asked_again_iff / asked_again_as_over_http / client_errors_stay_distinct / transport_errors_stay_distinct",
        axis: Lean,
        anchor: "crates/engine/vera-embedded/src/call.rs within; crates/engine/vera-embedded/src/deadline.rs bounded; crates/engine/vera-embedded/src/submission.rs client_error; crates/engine/vera-embedded/src/acp.rs transport_error; crates/engine/vera/src/vera_rs/client.rs ClientError::retryable",
        model_ref: "VeraEmbeddedTransport (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The embedded node streams only headers of blocks that finalize
        // after a subscription; a fresh deployment's first block finalized
        // before it, and a lone validator proposes the next only at its
        // heartbeat. The transport starts the stream with the newest
        // finalized block, read after the subscription stands, and the light
        // client checks it as it checks every header.
        family: "vera.rs embedded transport",
        name: "INV_OnlyFinalized / INV_Current / INV_SentHeadFresh: the light client takes only a header a finalized block certifies, once the stream is drained it stands on the newest fresh finalized block in its term without waiting for another, and the transport never sends a head too old to take",
        axis: Tla,
        anchor: "crates/engine/vera-embedded/src/acp.rs subscribe_headers, prepend_head; crates/engine/vera-embedded/src/acp/head.rs finalized, header, recent; backbone:acp-light-client HeaderChain::start",
        model_ref: "MC_FirstHeader_{Green,Red_NoHead,Red_ReadBeforeSubscribe,Red_SendStale,Red_DropLaterTerms,Red_Unverified}.cfg",
        tiers: &[Boundary],
        bound_by: "crates/engine/vera-embedded/src/acp/tests.rs queued_headers_survive_a_failed_head_authentication, a_header_of_a_later_term_at_or_below_the_fetched_head_is_emitted; crates/engine/vera-embedded/tests/embedded_node.rs a_fresh_deployments_provider_starts_on_its_first_block; crates/entry-points/defra-node/tests/embedded_vera/first_start.rs a_fresh_deployments_first_start_takes_its_first_block",
    },
];
