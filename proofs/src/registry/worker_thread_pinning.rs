use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "workers, edge";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // A pool and the network runtime it is given the same placement pin
        // by one rule: the default (spread on Linux) and every forced
        // placement pin, unpinned never does, and the auto opt-in pins only
        // where the process may run on part of the host's online CPUs (a
        // cpuset, taskset, systemd CPUAffinity), only within that set. Each
        // process's ledger is its own, so forced pinning on a shared host,
        // and auto on one narrowed set shared by several processes, pile up;
        // unpinned is the setting for processes that share their CPUs.
        family: "Worker thread pinning",
        name: "narrowed_iff / the_default_pins / forced_always_pins / unpinned_never_pins / unpinned_shares_nothing / all_cpu_mask_pins_nothing / shared_host_pins_nothing_in_common / forced_on_a_shared_host_piles_up / auto_on_equal_narrowed_masks_piles_up / narrowed_mask_pins / pins_within_mask / disjoint_masks_stay_apart / edge_pins_iff_pool_pins: the default pins, unpinned never pins, auto pins only within a narrowed mask, never outside the mask, and the network runtime pins exactly when the pool does",
        axis: Lean,
        anchor: "crates/foundation/workers/src/placement/mod.rs pins, plan; crates/foundation/workers/src/placement/reserve.rs reserve; crates/foundation/workers/src/placement/edge.rs EdgeCores::reserve_on, EdgeCores::reserve; crates/foundation/workers/src/config/mod.rs Placement::default; crates/foundation/workers/src/topology/linux.rs narrowed, detect; crates/foundation/edge/src/platform/native.rs own",
        model_ref: "PinningRule (lake build)",
        tiers: &[Contract],
        bound_by: "crates/foundation/workers/src/placement/pinning_tests.rs the_rule_matches_the_lean_model, auto_on_an_all_cpu_mask_pins_nothing, auto_on_a_shared_host_pins_no_cpu_in_common, auto_on_a_narrowed_mask_pins_within_it, auto_on_disjoint_narrowed_masks_keeps_processes_apart, auto_on_one_narrowed_mask_shared_by_two_processes_piles_up, a_forced_placement_pins_on_a_shared_host, unpinned_pins_nothing_even_on_a_narrowed_mask; crates/foundation/workers/src/topology/linux.rs a_set_narrowed_only_when_it_leaves_out_an_online_cpu, a_host_without_an_online_list_is_not_narrowed; crates/foundation/workers/tests/edge_cores.rs the_edge_pins_as_a_pool_placed_the_same_would, an_edge_that_does_not_pin_holds_nothing",
    },
];
