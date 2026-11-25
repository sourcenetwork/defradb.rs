use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A node checks its config before it opens anything, hands
        // its retry ladder to every retry site of either transport, stops a
        // host whose listen failed, enables NAC owned by the node identity,
        // and dials bootstrap peers only once NAC is in place, logging a peer
        // that does not answer.
        family: "Node startup",
        name: "INV_FailedHoldsNothing / INV_OneSchedule / INV_NacAsAsked / INV_NoPeerBeforeNac / INV_BootstrapNonFatal / LIVE_StartupEnds: a failed start holds nothing, a started node is configured as asked",
        axis: Tla,
        anchor: "crates/entry-points/defra-node/src/assemble.rs assemble; crates/entry-points/defra-node/src/libp2p_runtime.rs setup_libp2p, assemble_libp2p; crates/entry-points/defra-node/src/p2p_runtime.rs setup_iroh; crates/entry-points/defra-node/src/node_acp.rs create_nac_manager",
        model_ref: "MC_NodeStartup_{Green,Red_IrohDefaultLadder,Red_ListenLeak,Red_LateNacCheck,Red_NacUnowned,Red_DialBeforeNac,Red_BootstrapFatal}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Node startup",
        name: "new_keeps / new_valid / new_accepts_valid / cap_mem / cap_positive / cap_follows_ladder / cap_repeats_last / default_accepted",
        axis: Lean,
        anchor: "crates/engine/storage/src/stores/retry_info.rs RetrySchedule::new, RetrySchedule::cap, RETRY_INTERVALS_SECS",
        model_ref: "RetrySchedule (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
