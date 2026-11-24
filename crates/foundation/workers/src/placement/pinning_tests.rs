//! Whether a pool pins: the default and the other forced placements whatever
//! the set, the `Auto` opt-in only within a CPU set narrowed for the process,
//! `Unpinned` never. Each process has a ledger of its own, so two processes
//! on one host are two ledgers over the same machine.

use super::tests::{config, machine};
use super::*;
use crate::ledger::tests::ledger;

fn set(list: &str) -> CpuSet {
    CpuSet::parse(list).unwrap()
}

/// Every CPU `plan` pins a thread to: its workers' and the background its
/// other threads (compaction, blocking, edge) take.
fn pinned(plan: &Plan) -> CpuSet {
    let mut cpus = CpuSet::new();
    plan.seats
        .iter()
        .for_each(|seat| cpus.union_with(&seat.cpus));
    (0..plan.background.len().max(1))
        .for_each(|index| cpus.union_with(&plan.background_for(index).1));
    cpus
}

fn seat_cpus(plan: &Plan) -> Vec<String> {
    plan.seats.iter().map(|s| s.cpus.to_string()).collect()
}

/// The host every test runs on: 8 cores of two siblings, core `c` holding
/// CPUs `c` and `c + 8`, all of them online.
fn host() -> Topology {
    machine(1, 1, 8, 2)
}

/// The host as a process sees it when given `cpus` of its own.
fn given(cpus: &str) -> Topology {
    Topology {
        narrowed: true,
        ..host().within(&set(cpus))
    }
}

#[test]
fn auto_on_an_all_cpu_mask_pins_nothing() {
    for workers in [Workers::Auto, Workers::Count(3)] {
        let reserved = reserve(&host(), &config(workers.clone(), Placement::Auto), ledger());
        assert!(!reserved.plan.seats.is_empty());
        assert_eq!(
            pinned(&reserved.plan),
            CpuSet::new(),
            "{workers:?}: no worker, compaction or blocking thread is pinned"
        );
        assert!(reserved.hold.is_none(), "{workers:?}: no CPU is held");
    }
}

#[test]
fn auto_on_a_shared_host_pins_no_cpu_in_common() {
    let auto = config(Workers::Count(2), Placement::Auto);
    let first = reserve(&host(), &auto, ledger());
    let second = reserve(&host(), &auto, ledger());
    let both = pinned(&first.plan).intersection(&pinned(&second.plan));
    assert!(both.is_empty(), "both processes pin threads to CPUs {both}");
}

#[test]
fn auto_on_a_narrowed_mask_pins_within_it() {
    let process = given("0-3,8-11");
    let reserved = reserve(
        &process,
        &config(Workers::Count(2), Placement::Auto),
        ledger(),
    );
    assert_eq!(seat_cpus(&reserved.plan), vec!["0", "1"]);
    assert_eq!(
        reserved.plan.seats,
        plan(&process, &config(Workers::Count(2), Placement::Spread)).seats,
        "placed as spread"
    );
    let used = pinned(&reserved.plan);
    assert!(!used.is_empty());
    assert_eq!(used.intersection(&set("0-3,8-11")), used, "within the set");
    let held = reserved.hold.as_ref().map(|hold| hold.cpus().clone());
    assert!(
        held.as_ref()
            .is_some_and(|held| held.intersection(&set("0-3,8-11")) == *held),
        "{held:?} held within the set"
    );
}

#[test]
fn auto_on_disjoint_narrowed_masks_keeps_processes_apart() {
    let auto = config(Workers::Count(1), Placement::Auto);
    let first = reserve(&given("0-1,8-9"), &auto, ledger());
    let second = reserve(&given("2-3,10-11"), &auto, ledger());
    assert_eq!(seat_cpus(&first.plan), vec!["0"]);
    assert_eq!(seat_cpus(&second.plan), vec!["2"]);
    assert!(pinned(&first.plan)
        .intersection(&pinned(&second.plan))
        .is_empty());
}

/// A narrowed mask does not make its CPUs the process's alone: two
/// processes given the same set (one `--cpuset-cpus`, one systemd slice)
/// pin the same threads to the same CPUs under `Auto`, as two processes on a
/// whole host do under `Spread`.
#[test]
fn auto_on_one_narrowed_mask_shared_by_two_processes_piles_up() {
    let auto = config(Workers::Count(2), Placement::Auto);
    let first = reserve(&given("0-3,8-11"), &auto, ledger());
    let second = reserve(&given("0-3,8-11"), &auto, ledger());
    assert_eq!(seat_cpus(&first.plan), vec!["0", "1"]);
    assert_eq!(seat_cpus(&second.plan), vec!["0", "1"]);
    assert!(first.hold.is_some() && second.hold.is_some());
    let both = pinned(&first.plan).intersection(&pinned(&second.plan));
    assert_eq!(both, set("0-3,10-11"), "the same CPUs, pinned twice");
}

#[cfg(target_os = "linux")]
#[test]
fn the_default_pins_whatever_the_mask() {
    let default = config(Workers::Count(2), Placement::default());
    for process in [host(), given("0-3,8-11")] {
        let reserved = reserve(&process, &default, ledger());
        assert_eq!(seat_cpus(&reserved.plan), vec!["0", "1"]);
        assert!(reserved.hold.is_some());
    }
}

#[test]
fn a_forced_placement_pins_on_a_shared_host() {
    let shared = host();
    let spread = reserve(
        &shared,
        &config(Workers::Count(2), Placement::Spread),
        ledger(),
    );
    assert_eq!(seat_cpus(&spread.plan), vec!["0", "1"]);
    assert!(spread.hold.is_some());
    let pack = reserve(
        &shared,
        &config(Workers::Count(2), Placement::Pack),
        ledger(),
    );
    assert_eq!(seat_cpus(&pack.plan), vec!["0", "1"]);
    let custom = reserve(
        &shared,
        &config(Workers::Auto, Placement::Custom(vec![set("5")])),
        ledger(),
    );
    assert_eq!(seat_cpus(&custom.plan), vec!["5"]);
}

/// `Unpinned` is the setting for processes that share their CPUs, narrowed
/// or not: none of them pins a thread or holds a CPU.
#[test]
fn unpinned_pins_nothing_even_on_a_narrowed_mask() {
    let unpinned = config(Workers::Count(2), Placement::Unpinned);
    for process in [host(), given("0-3,8-11")] {
        let first = reserve(&process, &unpinned, ledger());
        let second = reserve(&process, &unpinned, ledger());
        for reserved in [&first, &second] {
            assert_eq!(pinned(&reserved.plan), CpuSet::new());
            assert!(reserved.hold.is_none());
        }
    }
}

/// The truth table of `PinningRule.pins` in `proofs/lean/PinningRule.lean`.
#[test]
fn the_rule_matches_the_lean_model() {
    let custom = Placement::Custom(vec![set("0")]);
    let table = [
        (Placement::Auto, false, false),
        (Placement::Auto, true, true),
        (Placement::Unpinned, false, false),
        (Placement::Unpinned, true, false),
        (Placement::Spread, false, true),
        (Placement::Spread, true, true),
        (Placement::Pack, false, true),
        (Placement::Pack, true, true),
        (custom.clone(), false, true),
        (custom, true, true),
    ];
    for (placement, narrowed, expected) in table {
        let topology = Topology {
            narrowed,
            ..machine(1, 1, 2, 1)
        };
        assert_eq!(
            pins(&topology, &placement),
            expected,
            "{placement:?}, narrowed {narrowed}"
        );
    }
}
