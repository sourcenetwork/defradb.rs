use super::*;
use crate::topology::{Node, Source};

/// A Linux-numbered machine: `nodes` nodes of `domains` L3 domains of `cores`
/// cores, `smt` siblings each; core `c`'s siblings are `c`, `c + total`, ...
pub(super) fn machine(nodes: usize, domains: usize, cores: usize, smt: usize) -> Topology {
    let total = nodes * domains * cores;
    let mut next = 0;
    let nodes = (0..nodes)
        .map(|node| {
            let mut list = Vec::new();
            for domain in 0..domains {
                for _ in 0..cores {
                    list.push(Core {
                        package: node,
                        l3: node * domains + domain,
                        kind: CoreKind::Unknown,
                        cpus: (0..smt).map(|s| next + s * total).collect(),
                    });
                    next += 1;
                }
            }
            Node {
                id: node,
                cores: list,
            }
        })
        .collect();
    Topology {
        nodes,
        source: Source::Fallback,
        cpu_quota: None,
    }
}

pub(super) fn config(workers: Workers, placement: Placement) -> Config {
    Config {
        workers,
        placement,
        ..Config::default()
    }
}

fn cpus(plan: &Plan) -> Vec<String> {
    plan.seats.iter().map(|s| s.cpus.to_string()).collect()
}

fn nodes(plan: &Plan) -> Vec<usize> {
    plan.seats.iter().map(|s| s.node).collect()
}

fn distinct(plan: &Plan) -> bool {
    let mut all: Vec<String> = cpus(plan);
    all.sort();
    all.dedup();
    all.len() == plan.seats.len()
}

#[test]
fn spread_alternates_nodes_then_l3_domains_on_first_siblings() {
    let plan = plan(
        &machine(2, 2, 2, 2),
        &config(Workers::Count(4), Placement::Spread),
    );
    assert_eq!(cpus(&plan), vec!["0", "4", "2", "6"]);
    assert_eq!(nodes(&plan), vec![0, 1, 0, 1]);
    assert!(!plan.oversubscribed);
    let background: Vec<(usize, String)> = plan
        .background
        .iter()
        .map(|(n, c)| (*n, c.to_string()))
        .collect();
    assert_eq!(
        background,
        vec![(0, "1,3,9,11".into()), (1, "5,7,13,15".into())]
    );
}

#[test]
fn pack_fills_one_node_before_the_next() {
    let plan = plan(
        &machine(2, 2, 2, 2),
        &config(Workers::Count(4), Placement::Pack),
    );
    assert_eq!(cpus(&plan), vec!["0", "1", "2", "3"]);
    assert_eq!(nodes(&plan), vec![0, 0, 0, 0]);
    let background: Vec<(usize, String)> = plan
        .background
        .iter()
        .map(|(n, c)| (*n, c.to_string()))
        .collect();
    assert_eq!(background, vec![(1, "4-7,12-15".into())]);
}

#[test]
fn siblings_are_used_only_after_every_core_has_a_worker() {
    let plan = plan(
        &machine(2, 2, 2, 2),
        &config(Workers::Count(10), Placement::Spread),
    );
    assert_eq!(&cpus(&plan)[8..], ["8", "12"]);
    assert!(distinct(&plan));
    assert!(!plan.oversubscribed);
    let background: Vec<(usize, String)> = plan
        .background
        .iter()
        .map(|(n, c)| (*n, c.to_string()))
        .collect();
    assert_eq!(
        background,
        vec![(0, "9-11".into()), (1, "13-15".into())],
        "no spare core: spare siblings"
    );
}

#[test]
fn smt_off_machines_place_one_worker_per_cpu() {
    let plan = plan(
        &machine(1, 1, 4, 1),
        &config(Workers::Count(4), Placement::Spread),
    );
    assert_eq!(cpus(&plan), vec!["0", "1", "2", "3"]);
    assert!(plan.background.is_empty());
}

#[test]
fn more_workers_than_cpus_share_cpus_and_say_so() {
    let plan = plan(
        &machine(2, 2, 2, 2),
        &config(Workers::Count(20), Placement::Spread),
    );
    assert_eq!(plan.seats.len(), 20);
    assert!(plan.oversubscribed);
    assert!(plan.notes.iter().any(|n| n.contains("20 workers on 16")));
    let first: Vec<String> = cpus(&plan)[..16].to_vec();
    let mut unique = first.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        16,
        "every CPU is used once before any is shared"
    );
}

#[test]
fn a_restricted_cpuset_is_all_the_plan_uses() {
    let mut topology = machine(1, 1, 3, 2);
    topology.nodes[0].cores[0].cpus = CpuSet::parse("0").unwrap();
    topology.nodes[0].cores[2].cpus = CpuSet::parse("2").unwrap();
    let plan = plan(&topology, &config(Workers::Count(3), Placement::Spread));
    assert_eq!(cpus(&plan), vec!["0", "1", "2"]);
    let allowed = topology.allowed();
    assert!(plan
        .seats
        .iter()
        .all(|s| s.cpus.iter().all(|c| allowed.contains(c))));
    assert_eq!(plan.background, vec![(0, CpuSet::parse("4").unwrap())]);
}

#[test]
fn auto_is_cores_minus_the_edge_and_compaction_within_the_quota() {
    let topology = machine(2, 2, 2, 2);
    assert_eq!(worker_count(&topology, &Config::default()), 8 - 2 - 1);
    let all = Config {
        smt: Smt::All,
        ..Config::default()
    };
    assert_eq!(worker_count(&topology, &all), 16 - 2 - 1);
    let none_kept = Config {
        edge_threads: Some(0),
        compaction_threads: 0,
        ..Config::default()
    };
    assert_eq!(worker_count(&topology, &none_kept), 8);
    let quota = Topology {
        cpu_quota: Some(4),
        ..topology.clone()
    };
    assert_eq!(worker_count(&quota, &Config::default()), 1);
    assert_eq!(
        worker_count(&Topology::single(1), &Config::default()),
        1,
        "never zero"
    );
}

#[test]
fn auto_on_a_heterogeneous_chip_is_its_performance_cores() {
    let mut topology = Topology::single(8);
    for (i, core) in topology.nodes[0].cores.iter_mut().enumerate() {
        core.kind = if i < 6 {
            CoreKind::Performance
        } else {
            CoreKind::Efficiency
        };
    }
    assert_eq!(worker_count(&topology, &Config::default()), 6);
}

#[test]
fn custom_sets_are_kept_restricted_to_allowed_cpus() {
    let topology = machine(2, 1, 2, 1);
    let sets = vec![
        CpuSet::parse("0-1").unwrap(),
        CpuSet::parse("3,9").unwrap(),
        CpuSet::parse("40").unwrap(),
    ];
    let plan = plan(&topology, &config(Workers::Auto, Placement::Custom(sets)));
    assert_eq!(cpus(&plan), vec!["0-1", "3", ""]);
    assert_eq!(nodes(&plan), vec![0, 1, 0]);
    assert!(plan.notes.iter().any(|n| n.contains("40")));
}

#[test]
fn unpinned_workers_have_no_cpus_and_leave_no_background() {
    let plan = plan(
        &machine(2, 1, 2, 2),
        &config(Workers::Count(3), Placement::Unpinned),
    );
    assert!(plan.seats.iter().all(|s| s.cpus.is_empty()));
    assert!(plan.background.is_empty());
    assert_eq!(plan.background_for(5), (0, CpuSet::new()));
}

#[test]
fn background_threads_take_nodes_in_turn() {
    let plan = plan(
        &machine(2, 2, 2, 2),
        &config(Workers::Count(4), Placement::Spread),
    );
    assert_eq!(plan.background_for(0).0, 0);
    assert_eq!(plan.background_for(1).0, 1);
    assert_eq!(plan.background_for(2).0, 0);
}
