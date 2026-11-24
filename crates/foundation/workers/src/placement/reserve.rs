//! A starting pool's share of the machine: its plan on the CPUs no other
//! placed pool holds, and its hold on them in the [`Ledger`].
//!
//! A pool holds whole cores, every SMT sibling of each core it puts a thread
//! on, so no other pool runs on the other half of one of its cores. Of the
//! spare cores its plan leaves, it keeps only as many as its own threads
//! need (the cores [`Workers::Auto`] sets aside for the edge and compaction,
//! at least one, node by node in turn); the rest stay free for other pools.
//!
//! [`Workers::Auto`] is sized from the free CPUs, within what is left of a
//! cgroup CPU quota once the CPUs other pools hold are counted against it.
//! An explicit count or custom CPU sets take the free CPUs when every worker
//! gets a CPU of its own there and the pool keeps a background wherever it
//! would alone. When they do not fit, nothing valid is refused: the pool is
//! planned as if it were alone, shares the CPUs the other pools hold, and
//! its plan says so (marked oversubscribed, with a note the start-up report
//! logs as a warning). With no CPU free at all, `Auto` is one unpinned
//! worker. A shared pool's CPUs are counted in the ledger too, so no later
//! pool takes them as free.

use super::{interleave, plan, set_aside, Plan, Seat};
use crate::config::{Config, Placement, Workers};
use crate::ledger::{Hold, Ledger};
use crate::topology::{CpuSet, Topology};

/// How often a pool plans again after other pools took CPUs it had planned
/// on, before it settles for sharing.
pub(super) const ATTEMPTS: usize = 16;

/// A pool's plan and the CPUs it holds for it.
pub(crate) struct Reserved<'a> {
    pub plan: Plan,
    /// `None` for a pool pinned nowhere.
    pub hold: Option<Hold<'a>>,
}

/// Plans a pool of `config` on the CPUs of `machine` that no pool in
/// `ledger` holds, and holds them.
pub(crate) fn reserve<'a>(machine: &Topology, config: &Config, ledger: Ledger<'a>) -> Reserved<'a> {
    if config.placement == Placement::Unpinned {
        return Reserved {
            plan: plan(machine, config),
            hold: None,
        };
    }
    let allowed = machine.allowed();
    let mut attempt = 0;
    loop {
        let free = ledger.free(&allowed);
        let held = allowed.len() - free.len();
        let rest = Topology {
            cpu_quota: machine.cpu_quota.map(|quota| quota.saturating_sub(held)),
            ..machine.within(&free)
        };
        let Some(fitted) = fit(machine, &rest, config) else {
            let why = format!(
                "{} of {} allowed CPUs are free of other pools, too few for this pool",
                free.len(),
                allowed.len()
            );
            return shared(machine, config, ledger, free.is_empty(), &why);
        };
        let cpus = cores_of(&fitted, &rest);
        if let Some(hold) = ledger.hold(&cpus) {
            return Reserved {
                plan: fitted,
                hold: Some(hold),
            };
        }
        attempt += 1;
        if attempt == ATTEMPTS {
            let mut plan = fitted;
            plan.oversubscribed = true;
            plan.notes.push(
                "other pools kept taking the free CPUs while this pool started: it may share CPUs with them"
                    .to_owned(),
            );
            return Reserved {
                plan,
                hold: Some(ledger.share(&cpus)),
            };
        }
    }
}

/// Whether `config` sizes the pool from the CPUs it finds.
fn sized_by_machine(config: &Config) -> bool {
    config.workers == Workers::Auto && !matches!(config.placement, Placement::Custom(_))
}

/// The pool planned on `rest` (`machine` less the CPUs other pools hold),
/// its spare cores bounded; `None` when its shape does not fit there.
fn fit(machine: &Topology, rest: &Topology, config: &Config) -> Option<Plan> {
    if rest.nodes.is_empty() {
        return None;
    }
    let mut planned = plan(rest, config);
    if !sized_by_machine(config) {
        let alone = plan(machine, config);
        let own_cpus = seated(&planned).len() == seated(&alone).len();
        let background = !planned.background.is_empty() || alone.background.is_empty();
        if !(own_cpus && background) {
            return None;
        }
    }
    keep_background(&mut planned, rest, config);
    Some(planned)
}

/// The pool planned as if alone, sharing the CPUs other pools hold, and
/// saying so; `Auto` with nothing free is one unpinned worker.
fn shared<'a>(
    machine: &Topology,
    config: &Config,
    ledger: Ledger<'a>,
    nothing_free: bool,
    why: &str,
) -> Reserved<'a> {
    if sized_by_machine(config) && nothing_free {
        let plan = Plan {
            seats: vec![Seat {
                node: 0,
                cpus: CpuSet::new(),
            }],
            background: Vec::new(),
            oversubscribed: true,
            notes: vec![format!("{why}: one worker runs unpinned beside them")],
        };
        return Reserved { plan, hold: None };
    }
    let mut planned = plan(machine, config);
    keep_background(&mut planned, machine, config);
    planned.oversubscribed = true;
    planned
        .notes
        .push(format!("{why}: it shares CPUs with them"));
    let hold = ledger.share(&cores_of(&planned, machine));
    Reserved {
        plan: planned,
        hold: Some(hold),
    }
}

/// The CPUs the workers sit on.
fn seated(plan: &Plan) -> CpuSet {
    let mut cpus = CpuSet::new();
    plan.seats
        .iter()
        .for_each(|seat| cpus.union_with(&seat.cpus));
    cpus
}

/// Every CPU of the cores `plan` puts a thread on.
fn cores_of(plan: &Plan, machine: &Topology) -> CpuSet {
    let mut used = seated(plan);
    plan.background
        .iter()
        .for_each(|(_, cpus)| used.union_with(cpus));
    let mut cores = CpuSet::new();
    machine
        .cores()
        .filter(|(_, core)| !core.cpus.intersection(&used).is_empty())
        .for_each(|(_, core)| cores.union_with(&core.cpus));
    cores
}

/// Keeps of the plan's spare whole cores only the ones its own threads use,
/// node by node in turn; spare siblings of the workers' own cores stay as
/// they are.
fn keep_background(plan: &mut Plan, machine: &Topology, config: &Config) {
    let keep = set_aside(machine, config).max(1);
    let seated = seated(plan);
    let spare: Vec<Vec<(usize, &CpuSet)>> = plan
        .background
        .iter()
        .map(|(node, cpus)| {
            machine
                .cores()
                .filter(|(at, core)| {
                    at.id == *node
                        && core.cpus.intersection(&seated).is_empty()
                        && !core.cpus.intersection(cpus).is_empty()
                })
                .map(|(_, core)| (*node, &core.cpus))
                .collect()
        })
        .collect();
    if spare.iter().all(Vec::is_empty) {
        return;
    }
    let mut kept: Vec<(usize, CpuSet)> = Vec::new();
    for (node, cpus) in interleave(spare).into_iter().take(keep) {
        match kept.iter_mut().find(|(at, _)| *at == node) {
            Some((_, set)) => set.union_with(cpus),
            None => kept.push((node, cpus.clone())),
        }
    }
    kept.sort_by_key(|(node, _)| *node);
    plan.background = kept;
}

#[cfg(test)]
mod tests {
    use super::super::tests::{config, machine};
    use super::*;
    use crate::ledger::tests::ledger;

    fn set(list: &str) -> CpuSet {
        CpuSet::parse(list).unwrap()
    }

    /// `workers` workers keeping one spare core for their other threads.
    fn lean(workers: Workers) -> Config {
        Config {
            edge_threads: Some(1),
            compaction_threads: 0,
            ..config(workers, Placement::Spread)
        }
    }

    fn holds(reserved: &Reserved<'_>) -> CpuSet {
        reserved
            .hold
            .as_ref()
            .map(|hold| hold.cpus().clone())
            .unwrap_or_default()
    }

    fn uses(reserved: &Reserved<'_>) -> CpuSet {
        let mut cpus = seated(&reserved.plan);
        reserved
            .plan
            .background
            .iter()
            .for_each(|(_, set)| cpus.union_with(set));
        cpus
    }

    #[test]
    fn placed_pools_take_disjoint_whole_cores_until_none_is_free() {
        // 2 nodes x 2 L3 domains x 2 cores x 2 siblings: core c is CPUs c, c+8.
        let machine = machine(2, 2, 2, 2);
        let ledger = ledger();
        let first = reserve(&machine, &lean(Workers::Count(2)), ledger);
        let second = reserve(&machine, &lean(Workers::Count(2)), ledger);
        let third = reserve(&machine, &lean(Workers::Auto), ledger);
        let pools = [&first, &second, &third];
        let mut all = CpuSet::new();
        for pool in pools {
            assert!(!pool.plan.oversubscribed, "{:?}", pool.plan.notes);
            let held = holds(pool);
            assert!(all.intersection(&held).is_empty(), "{held} is held twice");
            all.union_with(&held);
            let used = uses(pool);
            assert_eq!(
                used.intersection(&held),
                used,
                "a pool runs only on what it holds"
            );
            for cpu in held.iter() {
                let sibling = if cpu < 8 { cpu + 8 } else { cpu - 8 };
                assert!(held.contains(sibling), "core of CPU {cpu} is held whole");
            }
        }
        assert_eq!(seated(&first.plan).len(), 2);
        assert_eq!(
            first
                .plan
                .background
                .iter()
                .map(|(_, c)| c.len())
                .sum::<usize>(),
            2
        );
        assert_eq!(
            holds(&first).len(),
            6,
            "two worker cores and one spare core"
        );
        assert_eq!(holds(&second).len(), 6);
        assert_eq!(
            third.plan.seats.len(),
            1,
            "Auto: the two free cores less the one kept"
        );
        assert_eq!(all, machine.allowed(), "the machine is used up");
        assert_eq!(ledger.free(&machine.allowed()), CpuSet::new());
    }

    #[test]
    fn auto_with_no_cpu_free_is_one_unpinned_worker_and_says_so() {
        let machine = machine(1, 1, 2, 2);
        let ledger = ledger();
        let _all = reserve(&machine, &lean(Workers::Auto), ledger);
        let late = reserve(&machine, &lean(Workers::Auto), ledger);
        assert_eq!(late.plan.seats.len(), 1);
        assert!(late.plan.seats[0].cpus.is_empty(), "unpinned");
        assert!(late.hold.is_none());
        assert!(late.plan.oversubscribed);
        assert!(late
            .plan
            .notes
            .iter()
            .any(|n| n.contains("0 of 4 allowed CPUs are free")));
    }

    #[test]
    fn an_explicit_count_that_cannot_fit_shares_its_cpus_and_says_so() {
        let machine = machine(1, 1, 4, 1);
        let ledger = ledger();
        let first = reserve(&machine, &lean(Workers::Count(2)), ledger);
        assert_eq!(holds(&first), set("0-2"));
        let big = reserve(&machine, &lean(Workers::Count(2)), ledger);
        assert!(big.plan.oversubscribed);
        assert!(
            big.plan
                .notes
                .iter()
                .any(|n| n.contains("1 of 4 allowed CPUs are free")
                    && n.contains("it shares CPUs with them")),
            "{:?}",
            big.plan.notes
        );
        assert_eq!(
            seated(&big.plan).len(),
            2,
            "every worker keeps a CPU of its own"
        );
        drop(first);
        assert!(
            ledger
                .free(&machine.allowed())
                .intersection(&holds(&big))
                .is_empty(),
            "a sharing pool's CPUs stay held after the other pool is gone"
        );
        drop(big);
        assert_eq!(ledger.free(&machine.allowed()), machine.allowed());
    }

    #[test]
    fn a_pool_gone_gives_back_its_cores_to_the_next() {
        let machine = machine(1, 1, 4, 2);
        let ledger = ledger();
        let first = reserve(&machine, &lean(Workers::Count(2)), ledger);
        let taken = holds(&first);
        assert_eq!(taken, set("0-2,4-6"));
        drop(first);
        let next = reserve(&machine, &lean(Workers::Count(2)), ledger);
        assert_eq!(holds(&next), taken);
        assert!(!next.plan.oversubscribed);
    }

    #[test]
    fn a_pool_without_a_free_spare_core_where_it_would_have_one_shares() {
        let machine = machine(1, 1, 3, 1);
        let ledger = ledger();
        let _first = reserve(&machine, &lean(Workers::Count(1)), ledger);
        // One core is left: room for the worker, none for its other threads.
        let second = reserve(&machine, &lean(Workers::Count(1)), ledger);
        assert!(second.plan.oversubscribed);
        assert!(!second.plan.background.is_empty());
    }

    #[test]
    fn custom_sets_held_by_another_pool_are_shared_and_free_ones_are_held() {
        let machine = machine(1, 1, 4, 1);
        let ledger = ledger();
        let custom = |list: &str| Config {
            edge_threads: Some(1),
            compaction_threads: 0,
            ..config(Workers::Auto, Placement::Custom(vec![set(list)]))
        };
        let first = reserve(&machine, &custom("0"), ledger);
        assert_eq!(seated(&first.plan), set("0"));
        let clash = reserve(&machine, &custom("0"), ledger);
        assert!(clash.plan.oversubscribed);
        assert_eq!(seated(&clash.plan), set("0"));
        let elsewhere = reserve(&machine, &custom("3"), ledger);
        assert!(!elsewhere.plan.oversubscribed, "{:?}", elsewhere.plan.notes);
        assert!(holds(&elsewhere).contains(3));
    }

    #[test]
    fn auto_counts_other_pools_cpus_against_the_quota() {
        let machine = Topology {
            cpu_quota: Some(4),
            ..machine(1, 1, 8, 1)
        };
        let ledger = ledger();
        let first = reserve(&machine, &lean(Workers::Auto), ledger);
        assert_eq!(
            first.plan.seats.len(),
            3,
            "the quota of 4 less the edge's core"
        );
        let second = reserve(&machine, &lean(Workers::Auto), ledger);
        assert_eq!(
            second.plan.seats.len(),
            1,
            "the quota is used up: the least pool"
        );
        assert!(holds(&first).intersection(&holds(&second)).is_empty());
    }

    #[test]
    fn siblings_of_a_held_core_are_never_handed_to_another_pool() {
        // Core c is CPUs c and c + 4.
        let machine = machine(1, 1, 4, 2);
        let ledger = ledger();
        let first = reserve(&machine, &lean(Workers::Count(2)), ledger);
        assert_eq!(seated(&first.plan), set("0-1"));
        let second = reserve(&machine, &lean(Workers::Count(1)), ledger);
        assert_eq!(
            seated(&second.plan),
            set("3"),
            "not CPU 4 or 5, the other halves of the first pool's cores"
        );
        assert_eq!(
            second.plan.background,
            vec![(0, set("7"))],
            "its own core's sibling"
        );
        assert!(holds(&first).intersection(&holds(&second)).is_empty());
    }

    #[test]
    fn unpinned_pools_hold_nothing() {
        let machine = machine(1, 1, 2, 1);
        let ledger = ledger();
        let pool = reserve(
            &machine,
            &config(Workers::Count(4), Placement::Unpinned),
            ledger,
        );
        assert!(pool.hold.is_none());
        assert_eq!(ledger.free(&machine.allowed()), machine.allowed());
    }
}
