use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The replay-capability gate lives inside the P2P replication wire
        // protocol; presenting a forged/expired capability needs a custom peer,
        // not the public CLI. Rust-only hardening Go lacks; structural.
        family: "P2P explicit-replay capability gate",
        name: "INV_OnlyLegitAccepted — forged/expired/wrong-target capability rejected",
        axis: Tla,
        anchor: "crates/database/p2p capability replay gate",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The pending entry, its durable record across a restart, and the
        // merge (the collection/creator check and the strict-ACP bypass) reuse
        // the admission verdict without re-checking expiry or revocation.
        // Only KMS key release verifies again. Revocations live in process
        // memory and are lost on restart.
        family: "P2P explicit-replay capability gate",
        name: "INV_TtlCapped / INV_RevokedNeverAccepted hold at PushLog admission only",
        axis: Tla,
        anchor: "crates/database/db/src/merge/merge_handler/authorization.rs validate_explicit_replay_authorization; crates/database/p2p/src/explicit_replay.rs",
        model_ref: "MC_Capability_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // The capability is a field Go does not know, and Go verifies a
        // PushLog over the fields it knows: it travels only on Rust's
        // negotiated request protocol, never on Go's.
        family: "P2P explicit-replay capability gate",
        name: "INV_GoGetsOnlyGoFields / INV_NoPushDropped / INV_RustKeepsCapability",
        axis: Tla,
        anchor: "crates/database/p2p/src/two_stream/push_offer.rs PushLogOffer::sign, PushLogOffer::for_protocol; crates/database/p2p/src/two_stream/handler/pushlog.rs start_request",
        model_ref: "MC_PushLogGoWire_{Green,Red_AttachEverywhere,Red_StripEverywhere}.cfg",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/two_stream/handler/pushlog.rs retry_after_protocol_negotiates_and_falls_back_to_legacy",
    },
    Property {
        family: "P2P explicit-replay capability gate",
        name: "PushLogOffer.go_verifies_go_protocol / go_protocol_has_no_capability / rust_protocol_keeps_capability / rust_verifies_either / same_message_id",
        axis: Lean,
        anchor: "crates/database/p2p/src/two_stream/push_offer.rs PushLogOffer::sign, PushLogOffer::for_protocol",
        model_ref: "PushLogOffer.lean",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/two_stream/handler/pushlog.rs retry_after_protocol_negotiates_and_falls_back_to_legacy",
    },
];
