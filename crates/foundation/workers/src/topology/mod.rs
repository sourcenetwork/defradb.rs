//! The machine as the pool sees it: NUMA nodes, each holding physical cores,
//! each core holding its SMT siblings, with the package and L3 domain each
//! core belongs to. Read once at start and restricted to the CPUs the process
//! may run on.
//!
//! Linux reads sysfs and the process's affinity and cgroup CPU quota; macOS
//! reads `sysctl` (performance and efficiency cores, no NUMA); anything else,
//! or a Linux whose sysfs cannot be read, is one node of
//! `available_parallelism` single-CPU cores. Placement never affects
//! correctness, so the fallback is a slower placement, never a failure.

mod cpuset;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

pub use cpuset::{CpuSet, MAX_CPUS};

/// Where a [`Topology`] came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Linux sysfs.
    Sysfs,
    /// macOS `sysctl`.
    Sysctl,
    /// Nothing could be read: one node of `available_parallelism` CPUs.
    Fallback,
}

/// What kind of core a [`Core`] is, where the platform says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreKind {
    /// A performance core of a heterogeneous chip.
    Performance,
    /// An efficiency core of a heterogeneous chip.
    Efficiency,
    /// The platform does not say, or every core is alike.
    Unknown,
}

/// One physical core and the logical CPUs (SMT siblings) on it that the
/// process may run on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Core {
    /// The package (socket) the core sits in.
    pub package: usize,
    /// The L3 domain the core shares a cache with, named by the lowest CPU of
    /// the domain.
    pub l3: usize,
    /// Performance or efficiency, where the platform says.
    pub kind: CoreKind,
    /// The core's allowed logical CPUs, never empty.
    pub cpus: CpuSet,
}

/// A NUMA node and its allowed cores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// The node's id, as the kernel numbers it.
    pub id: usize,
    /// Its cores, ordered by their lowest CPU.
    pub cores: Vec<Core>,
}

/// The machine, restricted to what the process may use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    /// Nodes holding at least one allowed core, ordered by id.
    pub nodes: Vec<Node>,
    /// Where the tree came from.
    pub source: Source,
    /// The cgroup CPU quota, in whole CPUs rounded up, when one is set.
    pub cpu_quota: Option<usize>,
}

impl Topology {
    /// Reads the machine this process runs on.
    pub fn detect() -> Topology {
        #[cfg(target_os = "linux")]
        if let Some(topology) = linux::detect() {
            return topology;
        }
        #[cfg(target_os = "macos")]
        if let Some(topology) = macos::detect() {
            return topology;
        }
        Topology::single(std::thread::available_parallelism().map_or(1, |n| n.get()))
    }

    /// One node of `cpus` cores with one CPU each, numbered from 0: the
    /// fallback, and a convenient machine for tests.
    pub fn single(cpus: usize) -> Topology {
        let cores = (0..cpus.max(1))
            .map(|cpu| Core {
                package: 0,
                l3: 0,
                kind: CoreKind::Unknown,
                cpus: std::iter::once(cpu).collect(),
            })
            .collect();
        Topology {
            nodes: vec![Node { id: 0, cores }],
            source: Source::Fallback,
            cpu_quota: None,
        }
    }

    /// Every core, node by node.
    pub fn cores(&self) -> impl Iterator<Item = (&Node, &Core)> + '_ {
        self.nodes
            .iter()
            .flat_map(|node| node.cores.iter().map(move |core| (node, core)))
    }

    /// How many physical cores the process may use.
    pub fn core_count(&self) -> usize {
        self.nodes.iter().map(|n| n.cores.len()).sum()
    }

    /// How many logical CPUs the process may use.
    pub fn cpu_count(&self) -> usize {
        self.cores().map(|(_, core)| core.cpus.len()).sum()
    }

    /// How many packages hold an allowed core.
    pub fn package_count(&self) -> usize {
        self.distinct(|core| core.package)
    }

    /// How many L3 domains hold an allowed core.
    pub fn l3_count(&self) -> usize {
        self.distinct(|core| core.l3)
    }

    /// Every allowed CPU.
    pub fn allowed(&self) -> CpuSet {
        let mut all = CpuSet::new();
        self.cores()
            .for_each(|(_, core)| all.union_with(&core.cpus));
        all
    }

    /// This machine restricted to `cpus`: each core keeps only those of its
    /// CPUs, and cores and nodes left with none are dropped.
    pub(crate) fn within(&self, cpus: &CpuSet) -> Topology {
        let nodes = self
            .nodes
            .iter()
            .map(|node| Node {
                id: node.id,
                cores: node
                    .cores
                    .iter()
                    .map(|core| Core {
                        cpus: core.cpus.intersection(cpus),
                        ..core.clone()
                    })
                    .filter(|core| !core.cpus.is_empty())
                    .collect(),
            })
            .filter(|node| !node.cores.is_empty())
            .collect();
        Topology {
            nodes,
            ..self.clone()
        }
    }

    /// The node `cpu` belongs to, if it is allowed.
    pub fn node_of(&self, cpu: usize) -> Option<usize> {
        self.cores()
            .find(|(_, core)| core.cpus.contains(cpu))
            .map(|(node, _)| node.id)
    }

    fn distinct<F>(&self, key: F) -> usize
    where
        F: Fn(&Core) -> usize,
    {
        let mut seen: Vec<usize> = self.cores().map(|(_, core)| key(core)).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_is_one_node_of_single_cpu_cores() {
        let t = Topology::single(4);
        assert_eq!(t.nodes.len(), 1);
        assert_eq!((t.core_count(), t.cpu_count()), (4, 4));
        assert_eq!(t.allowed().to_string(), "0-3");
        assert_eq!(t.node_of(3), Some(0));
        assert_eq!(t.node_of(4), None);
        assert_eq!((t.package_count(), t.l3_count()), (1, 1));
    }

    #[test]
    fn zero_cpus_still_gives_one_core() {
        assert_eq!(Topology::single(0).core_count(), 1);
    }

    #[test]
    fn detect_finds_at_least_one_allowed_cpu_here() {
        let t = Topology::detect();
        assert!(t.cpu_count() >= 1);
        assert!(t.nodes.iter().all(|n| !n.cores.is_empty()));
        assert!(t.cores().all(|(_, c)| !c.cpus.is_empty()));
    }
}
