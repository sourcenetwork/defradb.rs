use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // The receive gate: a replication filter asked when a push arrives,
        // before its head is stored, and again on a fetched CAR, before it is
        // imported; the merge asks again as the backstop.
        family: "Receive gate",
        name: "INV_ArrivalRefusalStoresNothing / INV_FetchRefusalImportsNothing / INV_RefusedNeverMerged / INV_KeptIsKept",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_arrival, SyncCoordinator::gate_fetched; crates/database/db/src/receive_gate.rs DbReceiveGate::allows",
        model_ref: "MC_ReceiveGate_{Green,Red_*}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
    Property {
        // gate_fetched judges only roots with a resident pending record. Pulls
        // and expired unacknowledged entries take the Walk path: their CAR is
        // imported unchecked and refused at merge.
        family: "Receive gate",
        name: "INV_FetchRefusalImportsNothing holds for pushed roots only",
        axis: Tla,
        anchor: "crates/database/p2p/src/sync/coordinator/receive_gate.rs SyncCoordinator::gate_fetched",
        model_ref: "MC_ReceiveGate_Green.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
