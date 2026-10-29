//! Where each worker runs: a pure function of the topology and the
//! configuration, so it is tested on synthetic machines.
//!
//! Workers take one logical CPU each, in an order that depends on the
//! placement: spread alternates nodes and, within a node, L3 domains; pack
//! fills a node (an L3 domain at a time) before the next. Every core's first
//! sibling comes before any core's second, so physical cores are used before
//! SMT siblings. No two workers share a CPU unless more workers than allowed
//! CPUs were asked for, which the plan flags. CPUs no worker sits on (whole
//! spare cores first, spare siblings otherwise) are the background set the
//! compaction and blocking threads run on, and network threads placed
//! through the pool; a network runtime sized from the machine holds whole
//! cores of its own instead ([`EdgeCores`]).
//!
//! A pool that starts plans on the CPUs no other placed pool holds
//! ([`reserve`]), so pools never share a core.

#[cfg(not(target_family = "wasm"))]
mod edge;
#[cfg(not(target_family = "wasm"))]
mod reserve;
#[cfg(test)]
mod tests;

#[cfg(not(target_family = "wasm"))]
pub use edge::{edge_cores, EdgeCores};
#[cfg(not(target_family = "wasm"))]
pub(crate) use reserve::{reserve, Reserved};

use crate::config::{Config, Placement, Smt, Workers};
use crate::topology::{Core, CoreKind, CpuSet, Topology};

/// One worker's place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Seat {
    /// The node it counts as on, for routing.
    pub node: usize,
    /// The CPUs it is pinned to; empty when unpinned.
    pub cpus: CpuSet,
}

/// Every worker's place and what is left for other threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub seats: Vec<Seat>,
    /// Per node, the CPUs no worker sits on.
    pub background: Vec<(usize, CpuSet)>,
    /// More workers than allowed CPUs: some CPUs hold two workers.
    pub oversubscribed: bool,
    /// Things the report should say about the plan.
    pub notes: Vec<String>,
}

impl Plan {
    /// The background CPUs for the `index`th thread of a class: one node per
    /// thread, round robin. When nothing is left over, the workers' own CPUs,
    /// so a placed pool's threads stay on the CPUs it holds; empty
    /// (unpinned) for an unpinned pool.
    pub fn background_for(&self, index: usize) -> (usize, CpuSet) {
        if self.background.is_empty() {
            let mut seated = CpuSet::new();
            self.seats
                .iter()
                .for_each(|seat| seated.union_with(&seat.cpus));
            let node = self.seats.first().map_or(0, |seat| seat.node);
            return (node, seated);
        }
        self.background[index % self.background.len()].clone()
    }
}

/// How many workers `config` asks for on `topology`.
pub(crate) fn worker_count(topology: &Topology, config: &Config) -> usize {
    if let Placement::Custom(sets) = &config.placement {
        return sets.len();
    }
    let Workers::Count(n) = config.workers else {
        return auto_count(topology, config);
    };
    n.max(1)
}

fn auto_count(topology: &Topology, config: &Config) -> usize {
    let units = |core: &Core| match config.smt {
        Smt::PhysicalOnly => 1,
        Smt::All => core.cpus.len(),
    };
    let performance: usize = topology
        .cores()
        .filter(|(_, core)| core.kind == CoreKind::Performance)
        .map(|(_, core)| units(core))
        .sum();
    if performance > 0 {
        return performance;
    }
    let all: usize = topology.cores().map(|(_, core)| units(core)).sum();
    let usable = topology.cpu_quota.map_or(all, |quota| all.min(quota));
    usable.saturating_sub(set_aside(topology, config)).max(1)
}

/// The cores a pool keeps for its own threads besides the workers: the
/// network edge's (unless configured, none while a network runtime holds
/// cores of its own, else one per node, at least two) and compaction's.
/// [`Workers::Auto`] leaves them out of its count.
fn set_aside(topology: &Topology, config: &Config) -> usize {
    let edge = config.edge_threads.unwrap_or_else(|| {
        if edge_holds_cores() {
            0
        } else {
            topology.nodes.len().max(2)
        }
    });
    edge + config.compaction_threads
}

#[cfg(not(target_family = "wasm"))]
fn edge_holds_cores() -> bool {
    edge::holding()
}

#[cfg(target_family = "wasm")]
fn edge_holds_cores() -> bool {
    false
}

/// Every worker's place on `topology` under `config`.
pub(crate) fn plan(topology: &Topology, config: &Config) -> Plan {
    let count = worker_count(topology, config);
    let mut notes = Vec::new();
    let (seats, oversubscribed) = match &config.placement {
        Placement::Unpinned => (
            vec![
                Seat {
                    node: 0,
                    cpus: CpuSet::new()
                };
                count
            ],
            false,
        ),
        Placement::Custom(sets) => (custom(topology, sets, &mut notes), false),
        Placement::Spread | Placement::Pack => {
            let slots = slots(topology, config.placement == Placement::Spread);
            let oversubscribed = count > slots.len();
            let seats = slots
                .iter()
                .cycle()
                .take(count)
                .map(|(node, cpu)| Seat {
                    node: *node,
                    cpus: std::iter::once(*cpu).collect(),
                })
                .collect();
            (seats, oversubscribed)
        }
    };
    if oversubscribed {
        notes.push(format!(
            "{count} workers on {} allowed CPUs: some CPUs run two workers",
            topology.cpu_count()
        ));
    }
    let background = background(topology, &seats);
    Plan {
        seats,
        background,
        oversubscribed,
        notes,
    }
}

/// Each custom set restricted to the allowed CPUs; a set with none left runs
/// unpinned.
fn custom(topology: &Topology, sets: &[CpuSet], notes: &mut Vec<String>) -> Vec<Seat> {
    let allowed = topology.allowed();
    sets.iter()
        .map(|set| {
            let cpus = set.intersection(&allowed);
            if cpus.is_empty() {
                notes.push(format!(
                    "custom CPU set {set} holds no allowed CPU: that worker runs unpinned"
                ));
            }
            let node = cpus
                .first()
                .and_then(|cpu| topology.node_of(cpu))
                .unwrap_or(0);
            Seat { node, cpus }
        })
        .collect()
}

/// Every allowed CPU as `(node, cpu)`, in the order workers take them: every
/// core's first sibling, then every core's second, and so on, with cores
/// ordered by [`core_order`].
fn slots(topology: &Topology, spread: bool) -> Vec<(usize, usize)> {
    let cores = core_order(topology, spread);
    let siblings: Vec<Vec<usize>> = cores
        .iter()
        .map(|(_, core)| core.cpus.iter().collect())
        .collect();
    let deepest = siblings.iter().map(Vec::len).max().unwrap_or(0);
    (0..deepest)
        .flat_map(|round| {
            cores
                .iter()
                .zip(&siblings)
                .filter_map(move |((node, _), cpus)| cpus.get(round).map(|cpu| (*node, *cpu)))
        })
        .collect()
}

/// Cores in placement order. Spread alternates nodes, and within a node
/// alternates L3 domains; pack takes a node's L3 domains one after another,
/// node after node.
fn core_order(topology: &Topology, spread: bool) -> Vec<(usize, &Core)> {
    let per_node: Vec<Vec<(usize, &Core)>> = topology
        .nodes
        .iter()
        .map(|node| {
            let mut domains: Vec<usize> = node.cores.iter().map(|core| core.l3).collect();
            domains.sort_unstable();
            domains.dedup();
            let groups: Vec<Vec<(usize, &Core)>> = domains
                .iter()
                .map(|l3| {
                    node.cores
                        .iter()
                        .filter(|core| core.l3 == *l3)
                        .map(|core| (node.id, core))
                        .collect()
                })
                .collect();
            if spread {
                interleave(groups)
            } else {
                groups.into_iter().flatten().collect()
            }
        })
        .collect();
    if spread {
        interleave(per_node)
    } else {
        per_node.into_iter().flatten().collect()
    }
}

/// One from each list in turn until all are used up.
fn interleave<T>(lists: Vec<Vec<T>>) -> Vec<T> {
    let total = lists.iter().map(Vec::len).sum();
    let mut iters: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        for iter in &mut iters {
            if let Some(item) = iter.next() {
                out.push(item);
            }
        }
    }
    out
}

/// Per node, the CPUs of cores no worker sits on; when every core holds a
/// worker, the siblings no worker sits on.
fn background(topology: &Topology, seats: &[Seat]) -> Vec<(usize, CpuSet)> {
    let mut used = CpuSet::new();
    seats.iter().for_each(|seat| used.union_with(&seat.cpus));
    if used.is_empty() {
        return Vec::new();
    }
    let spare = |whole_cores: bool| -> Vec<(usize, CpuSet)> {
        topology
            .nodes
            .iter()
            .map(|node| {
                let mut free = CpuSet::new();
                for core in &node.cores {
                    let busy = !core.cpus.intersection(&used).is_empty();
                    if !busy {
                        free.union_with(&core.cpus);
                    } else if !whole_cores {
                        let mut left = core.cpus.clone();
                        left.remove_all(&used);
                        free.union_with(&left);
                    }
                }
                (node.id, free)
            })
            .filter(|(_, free)| !free.is_empty())
            .collect()
    };
    let cores = spare(true);
    if cores.is_empty() {
        spare(false)
    } else {
        cores
    }
}
