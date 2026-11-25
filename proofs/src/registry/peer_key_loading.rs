use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A peer key the caller hands in for one start (Go's P2PPrivateKey)
        // is used as given: never checked against the store and never
        // recorded in it, so a later start without it carries on with the
        // store's own key.
        family: "Peer key loading",
        name: "INV_StartKeyStarts / INV_StartKeyLeavesNoMark / INV_PlainKeyStable: a start with a start key always starts, leaves no mark that refuses a later plain start, and never changes the key plain starts use",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/peer_key.rs load_with_keyring, load_or_create; crates/entry-points/defra-node/src/start_peer_key.rs StartPeerKey; crates/entry-points/defra-node/src/libp2p_runtime.rs assemble_libp2p; crates/entry-points/defra-node/src/p2p_runtime.rs setup_iroh; crates/entry-points/ffi/src/node/mod.rs build_node_state",
        model_ref: "MC_PeerKeyLoad_{Green,Red_StartKeyChecked,Red_StartKeyRecorded}.cfg",
        tiers: &[Contract],
        bound_by: "crates/entry-points/defra-node/tests/unit/peer_keyring.rs a_start_key_is_used_as_given_and_leaves_a_plain_store_as_it_was, a_start_key_starts_a_keyring_store_without_changing_it",
    },
    Property {
        family: "Peer key loading",
        name: "start_key_is_used / start_key_leaves_store / plain_after_start_key: a start key is answered as given, the store comes back unchanged, and a plain start after it loads what it would have without it",
        axis: Lean,
        anchor: "crates/entry-points/defra-node/src/peer_key.rs load_with_keyring, load_or_create",
        model_ref: "PeerKeyLoad (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
