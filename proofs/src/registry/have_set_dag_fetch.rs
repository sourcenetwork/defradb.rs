use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Have-set DAG fetch",
        name: "INV_MergeComplete / INV_NoForged / INV_HeldNotResent / INV_RoundsBounded / INV_Converges",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/car/region.rs collect_missing_region, Walk::mark_held, Walk::mark_wanted, Walk::serve_attachments; crates/database/p2p/src/sync/coordinator/haves.rs haves_from_heads; crates/database/p2p/src/sync/coordinator/dag_fetcher.rs fetch_dag_attempt, root_haves; crates/database/p2p/src/sync/coordinator/event_handler/car.rs drop_held_blocks",
        model_ref: "MC_ExactDagFetch_{Green,Green_Liar,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Have-set DAG fetch",
        name: "INV_OneMarkWriter / INV_HeldNeverContends / INV_Lands",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/event_handler/car.rs drop_held_blocks; crates/database/p2p/src/sync/manager/process/mod.rs try_acquire_car_storage_owners; crates/engine/storage/src/stores/blockstore.rs BlockWritesAttempt",
        model_ref: "MC_HeldBlockIngest_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        family: "Have-set DAG fetch",
        name: "wanted_served / served_held_is_asked / missing_is_served / receiver_complete / reach_lowers_priority / first_is_root / decode_encode",
        axis: Lean,
        anchor: "crates/database/p2p/src/sync/car/region.rs collect_missing_region; crates/database/p2p/src/two_stream/handler/car.rs encode_car_request, decode_car_request",
        model_ref: "CarMissingRegion (lake build)",
        tiers: &[Boundary],
        bound_by: "",
    },
];
