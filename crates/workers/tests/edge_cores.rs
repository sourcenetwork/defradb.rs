use workers::topology::{Core, CoreKind, CpuSet, Node, Source, Topology};
use workers::{edge_cores, Config, EdgeCores, Pool, Priorities};

/// `nodes` nodes of `cores` cores of `smt` siblings, numbered from `base` so
/// machines of different tests never meet in the process's ledger.
fn machine(base: usize, nodes: usize, cores: usize, smt: usize) -> Topology {
    let total = nodes * cores;
    let nodes = (0..nodes)
        .map(|node| Node {
            id: node,
            cores: (0..cores)
                .map(|core| {
                    let first = base + node * cores + core;
                    Core {
                        package: node,
                        l3: base + node * cores,
                        kind: CoreKind::Unknown,
                        cpus: (0..smt).map(|s| first + s * total).collect(),
                    }
                })
                .collect(),
        })
        .collect();
    Topology {
        nodes,
        source: Source::Fallback,
        cpu_quota: None,
    }
}

fn set(list: &str) -> CpuSet {
    CpuSet::parse(list).unwrap()
}

#[test]
fn edge_cores_are_a_third_of_the_usable_cores_at_least_one_per_node() {
    assert_eq!(edge_cores(&machine(0, 1, 18, 2)), 6);
    assert_eq!(edge_cores(&machine(0, 1, 2, 1)), 1);
    assert_eq!(edge_cores(&machine(0, 1, 1, 1)), 1);
    assert_eq!(edge_cores(&machine(0, 2, 2, 1)), 2, "one per node");
    let quota = Topology {
        cpu_quota: Some(9),
        ..machine(0, 1, 18, 2)
    };
    assert_eq!(edge_cores(&quota), 3);
}

#[test]
fn a_reservation_holds_whole_cores_from_the_far_end_and_gives_them_back() {
    let host = machine(1000, 1, 9, 2);
    let first = EdgeCores::reserve_on(&host);
    assert!(first.is_pinned());
    assert_eq!(
        first.cpus(),
        &set("1006-1008,1015-1017"),
        "the last 3 cores, both siblings"
    );
    assert_eq!(first.threads(), 6, "one thread per CPU held");

    let second = EdgeCores::reserve_on(&host);
    assert_eq!(
        second.cpus(),
        &set("1004-1005,1013-1014"),
        "a third of what is free"
    );
    assert!(second.cpus().intersection(first.cpus()).is_empty());

    drop((first, second));
    let third = EdgeCores::reserve_on(&host);
    assert_eq!(
        third.cpus(),
        &set("1006-1008,1015-1017"),
        "the given-back cores again"
    );
}

#[test]
fn efficiency_cores_are_taken_first() {
    let mut host = machine(2000, 1, 6, 1);
    host.nodes[0].cores[1].kind = CoreKind::Efficiency;
    host.nodes[0].cores[2].kind = CoreKind::Efficiency;
    let cores = EdgeCores::reserve_on(&host);
    assert_eq!(cores.cpus(), &set("2001-2002"));
}

#[test]
fn every_node_gets_an_edge_core() {
    let host = machine(3000, 2, 3, 1);
    let cores = EdgeCores::reserve_on(&host);
    let nodes: Vec<_> = cores
        .cpus()
        .iter()
        .map(|cpu| host.node_of(cpu).unwrap())
        .collect();
    assert_eq!(nodes.len(), 2);
    assert_ne!(nodes[0], nodes[1]);
}

#[test]
fn a_machine_with_no_core_to_spare_runs_unpinned_holding_nothing() {
    let host = machine(4000, 1, 1, 2);
    let cores = EdgeCores::reserve_on(&host);
    assert!(!cores.is_pinned());
    assert_eq!(cores.threads(), 2, "one per CPU of its would-be cores");
    assert!(!cores.pin_current_thread());
}

#[test]
fn an_explicit_count_runs_unpinned() {
    let cores = EdgeCores::unpinned(std::num::NonZeroUsize::new(3).unwrap());
    assert_eq!((cores.threads(), cores.is_pinned()), (3, false));
}

#[cfg(target_os = "linux")]
#[test]
fn a_pool_plans_its_workers_around_the_edge_and_sets_nothing_aside_for_it() {
    let host = Topology::detect();
    let edge = EdgeCores::reserve_on(&host);
    if !edge.is_pinned() {
        return;
    }
    let config = Config {
        priorities: Priorities::inherited(),
        ..Config::default()
    };
    let compaction = config.compaction_threads;
    let pool = Pool::start(config).unwrap();
    let report = pool.report();
    for worker in &report.workers {
        if let Some(cpus) = worker.placement.pinning.granted.strip_prefix("CPUs ") {
            let cpus = CpuSet::parse(cpus).unwrap();
            assert!(
                cpus.intersection(edge.cpus()).is_empty(),
                "worker on {cpus}, edge on {}",
                edge.cpus()
            );
        }
    }
    let plain = host.cores().all(|(_, core)| core.kind == CoreKind::Unknown);
    if plain && host.cpu_quota.is_none() {
        let edge_cores = host
            .cores()
            .filter(|(_, core)| !core.cpus.intersection(edge.cpus()).is_empty())
            .count();
        let free = host.core_count() - edge_cores;
        assert_eq!(
            report.workers.len(),
            free.saturating_sub(compaction).max(1),
            "no cores kept for an edge that holds its own"
        );
    }
    pool.shutdown_blocking().unwrap();
}
