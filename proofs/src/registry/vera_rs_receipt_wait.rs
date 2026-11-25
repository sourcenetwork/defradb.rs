use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Between asks for a receipt the provider waits for the light
        // client to verify a height above the one it read before it asked
        // and sent, at most a fallback interval. The node serves a receipt
        // from the moment it publishes its block's header, so the receipt
        // is asked for as soon as it can be there; a header verified while
        // the ask or the send was on its way still ends the wait; and a
        // header stream that is down delays the receipt by the fallback,
        // never for good.
        family: "vera.rs receipt wait",
        name: "INV_Prompt / LIVE_Confirms: a receipt is asked for at the verified height that holds it, with no wait on a timer, and is read in the end when no header comes",
        axis: Tla,
        anchor: "crates/engine/vera/src/vera_rs/receipt_wait.rs until_receipt, Heads, Fallback, MIN_FALLBACK; crates/engine/vera/src/vera_rs/provider.rs receipt_fallbacks; crates/engine/vera/src/vera_rs/provider/submission.rs resolve_pending; crates/engine/vera/src/vera_rs/follow/chain.rs ProviderChain::send_and_confirm; crates/engine/vera/src/tuning.rs receipt_fallback",
        model_ref: "MC_ReceiptWait_{Green,Red_TimerOnly,Red_SeenAfterSend,Red_Unbounded}.cfg",
        tiers: &[Boundary, Contract],
        bound_by: "crates/engine/vera/src/vera_rs/receipt_wait_tests.rs a_receipt_is_asked_for_at_the_next_verified_height_and_not_after_a_sleep, a_header_verified_while_the_request_is_sent_still_ends_the_wait, with_no_header_each_wait_ends_at_its_fallback_and_an_unheard_send_is_sent_again; crates/engine/vera-embedded/tests/receipt_wait.rs writes_are_confirmed_at_their_blocks_header_and_not_at_the_fallback",
    },
];
