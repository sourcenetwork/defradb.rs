//! Linux: the tree from sysfs, restricted to `sched_getaffinity`'s set (which
//! already reflects cpusets and container limits), and the cgroup CPU quota
//! (`cpu.max` on cgroup v2, `cpu.cfs_quota_us` on v1).

use std::fs;
use std::path::{Path, PathBuf};

use super::{Core, CoreKind, CpuSet, Node, Source, Topology};

pub(super) fn detect() -> Option<Topology> {
    let allowed = crate::os::current_affinity()?;
    let nodes = read(Path::new("/sys"), &allowed)?;
    Some(Topology {
        nodes,
        source: Source::Sysfs,
        cpu_quota: cgroup_quota(Path::new("/proc/self/cgroup"), Path::new("/sys/fs/cgroup")),
    })
}

/// The nodes and cores under `sys` (normally `/sys`) holding an online CPU of
/// `allowed`. `None` when sysfs has no CPU directory to read.
pub(super) fn read(sys: &Path, allowed: &CpuSet) -> Option<Vec<Node>> {
    let cpu_dir = sys.join("devices/system/cpu");
    if !cpu_dir.is_dir() {
        return None;
    }
    let allowed = &match read_list(&cpu_dir.join("online")) {
        Some(online) => allowed.intersection(&online),
        None => allowed.clone(),
    };
    if allowed.is_empty() {
        return None;
    }
    let node_cpus = node_map(sys, allowed);
    let mut nodes: Vec<Node> = node_cpus
        .iter()
        .map(|(id, _)| Node {
            id: *id,
            cores: Vec::new(),
        })
        .collect();
    let mut seen = CpuSet::new();
    for cpu in allowed.iter() {
        if seen.contains(cpu) {
            continue;
        }
        let topo = cpu_dir.join(format!("cpu{cpu}/topology"));
        let mut cpus = read_list(&topo.join("core_cpus_list"))
            .or_else(|| read_list(&topo.join("thread_siblings_list")))
            .unwrap_or_default()
            .intersection(allowed);
        cpus.insert(cpu);
        seen.union_with(&cpus);
        let package = read_number(&topo.join("physical_package_id")).unwrap_or(0);
        let l3 = l3_domain(&cpu_dir.join(format!("cpu{cpu}/cache")))
            .or_else(|| read_list(&topo.join("package_cpus_list")).and_then(|p| p.first()))
            .unwrap_or(0);
        let node = node_cpus
            .iter()
            .position(|(_, set)| set.contains(cpu))
            .unwrap_or(0);
        nodes[node].cores.push(Core {
            package,
            l3,
            kind: CoreKind::Unknown,
            cpus,
        });
    }
    nodes.retain(|node| !node.cores.is_empty());
    nodes
        .iter_mut()
        .for_each(|node| node.cores.sort_by_key(|core| core.cpus.first()));
    Some(nodes)
}

/// Each node's id and allowed CPUs, by id. One node 0 holding everything when
/// the kernel has no node directory (built without NUMA).
fn node_map(sys: &Path, allowed: &CpuSet) -> Vec<(usize, CpuSet)> {
    let mut nodes: Vec<(usize, CpuSet)> = fs::read_dir(sys.join("devices/system/node"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str()?.strip_prefix("node")?.parse().ok()?;
            Some((
                id,
                read_list(&entry.path().join("cpulist"))?.intersection(allowed),
            ))
        })
        .collect();
    nodes.sort_by_key(|(id, _)| *id);
    if nodes.iter().all(|(_, cpus)| cpus.is_empty()) {
        return vec![(0, allowed.clone())];
    }
    nodes
}

/// The L3 domain of a CPU, named by the lowest CPU sharing it: the cache
/// index whose `level` is 3.
fn l3_domain(cache: &Path) -> Option<usize> {
    fs::read_dir(cache)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("index"))
        })
        .find(|entry| read_number(&entry.path().join("level")) == Some(3))
        .and_then(|entry| read_list(&entry.path().join("shared_cpu_list")))
        .and_then(|cpus| cpus.first())
}

fn read_list(path: &Path) -> Option<CpuSet> {
    CpuSet::parse(&fs::read_to_string(path).ok()?).ok()
}

fn read_number(path: &Path) -> Option<usize> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// The tightest CPU quota on the process's cgroup and its ancestors, in whole
/// CPUs rounded up. `proc_cgroup` is `/proc/self/cgroup`, `root` the cgroup
/// mount.
pub(super) fn cgroup_quota(proc_cgroup: &Path, root: &Path) -> Option<usize> {
    let membership = fs::read_to_string(proc_cgroup).ok()?;
    let mut tightest: Option<usize> = None;
    let mut tighten = |quota: Option<usize>| {
        if let Some(q) = quota {
            tightest = Some(tightest.map_or(q, |t| t.min(q)));
        }
    };
    for line in membership.lines() {
        let mut fields = line.splitn(3, ':');
        let (Some(_), Some(controllers), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if controllers.is_empty() {
            for dir in ancestors(root, path) {
                tighten(
                    fs::read_to_string(dir.join("cpu.max"))
                        .ok()
                        .and_then(|t| parse_cpu_max(&t)),
                );
            }
        } else if controllers.split(',').any(|c| c == "cpu") {
            let Some(mount) = ["cpu,cpuacct", "cpu", "cpuacct,cpu"]
                .iter()
                .map(|m| root.join(m))
                .find(|m| m.is_dir())
            else {
                continue;
            };
            for dir in ancestors(&mount, path) {
                let quota = fs::read_to_string(dir.join("cpu.cfs_quota_us")).ok();
                let period = fs::read_to_string(dir.join("cpu.cfs_period_us")).ok();
                if let (Some(quota), Some(period)) = (quota, period) {
                    tighten(parse_quota(&quota, &period));
                }
            }
        }
    }
    tightest
}

/// `root/path` and each of its parents up to `root` itself.
fn ancestors(root: &Path, path: &str) -> Vec<PathBuf> {
    let mut dir = root.join(path.trim_start_matches('/'));
    let mut all = vec![dir.clone()];
    while dir != root && dir.pop() && dir.starts_with(root) {
        all.push(dir.clone());
    }
    all
}

/// cgroup v2's `cpu.max`: `max <period>` is no quota, `<quota> <period>` is
/// `quota / period` CPUs, rounded up.
pub(super) fn parse_cpu_max(text: &str) -> Option<usize> {
    let mut fields = text.split_whitespace();
    parse_quota(fields.next()?, fields.next().unwrap_or("100000"))
}

/// A quota and a period in microseconds as whole CPUs, rounded up; `max` or a
/// negative quota (v1's "unlimited") is no quota.
fn parse_quota(quota: &str, period: &str) -> Option<usize> {
    let quota: i64 = quota.trim().parse().ok()?;
    let period: i64 = period.trim().parse().ok()?;
    if quota <= 0 || period <= 0 {
        return None;
    }
    usize::try_from((quota + period - 1) / period)
        .ok()
        .map(|cpus| cpus.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A scratch directory removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "workers-sysfs-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Two nodes, two cores each, two SMT siblings per core (cpu N and N+4),
    /// one package and one L3 per node.
    fn two_node_machine() -> Scratch {
        let sys = Scratch::new();
        sys.write("devices/system/node/node0/cpulist", "0-1,4-5\n");
        sys.write("devices/system/node/node1/cpulist", "2-3,6-7\n");
        sys.write("devices/system/node/online", "0-1\n");
        for cpu in 0..8 {
            let core = cpu % 4;
            let node = core / 2;
            let topo = format!("devices/system/cpu/cpu{cpu}/topology");
            sys.write(
                &format!("{topo}/core_cpus_list"),
                &format!("{core},{}\n", core + 4),
            );
            sys.write(&format!("{topo}/physical_package_id"), &format!("{node}\n"));
            let cache = format!("devices/system/cpu/cpu{cpu}/cache");
            sys.write(&format!("{cache}/index0/level"), "1\n");
            sys.write(
                &format!("{cache}/index0/shared_cpu_list"),
                &format!("{core},{}\n", core + 4),
            );
            sys.write(&format!("{cache}/index3/level"), "3\n");
            let l3 = if node == 0 { "0-1,4-5" } else { "2-3,6-7" };
            sys.write(&format!("{cache}/index3/shared_cpu_list"), l3);
        }
        sys
    }

    #[test]
    fn reads_nodes_cores_siblings_packages_and_l3_domains() {
        let sys = two_node_machine();
        let nodes = read(&sys.0, &CpuSet::parse("0-7").unwrap()).unwrap();
        assert_eq!(nodes.len(), 2);
        type Summary = (usize, Vec<String>, Vec<usize>, Vec<usize>);
        let summary: Vec<Summary> = nodes
            .iter()
            .map(|n| {
                (
                    n.id,
                    n.cores.iter().map(|c| c.cpus.to_string()).collect(),
                    n.cores.iter().map(|c| c.package).collect(),
                    n.cores.iter().map(|c| c.l3).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (0, vec!["0,4".into(), "1,5".into()], vec![0, 0], vec![0, 0]),
                (1, vec!["2,6".into(), "3,7".into()], vec![1, 1], vec![2, 2]),
            ]
        );
    }

    #[test]
    fn the_affinity_set_restricts_cores_siblings_and_nodes() {
        let sys = two_node_machine();
        let nodes = read(&sys.0, &CpuSet::parse("0,1,5").unwrap()).unwrap();
        assert_eq!(nodes.len(), 1, "node 1 has no allowed CPU");
        let cores: Vec<String> = nodes[0].cores.iter().map(|c| c.cpus.to_string()).collect();
        assert_eq!(cores, vec!["0", "1,5"]);
    }

    #[test]
    fn offline_cpus_are_skipped() {
        let sys = two_node_machine();
        sys.write("devices/system/cpu/online", "0-2,4-6\n");
        let nodes = read(&sys.0, &CpuSet::parse("0-7").unwrap()).unwrap();
        let cpus: Vec<String> = nodes
            .iter()
            .flat_map(|n| n.cores.iter().map(|c| c.cpus.to_string()))
            .collect();
        assert_eq!(
            cpus,
            vec!["0,4", "1,5", "2,6"],
            "core 3 (cpus 3 and 7) is offline"
        );
    }

    #[test]
    fn a_kernel_without_numa_is_one_node() {
        let sys = Scratch::new();
        sys.write("devices/system/cpu/cpu0/topology/core_cpus_list", "0\n");
        sys.write("devices/system/cpu/cpu1/topology/core_cpus_list", "1\n");
        let nodes = read(&sys.0, &CpuSet::parse("0-1").unwrap()).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].cores.len(), 2);
    }

    #[test]
    fn no_sysfs_is_no_topology() {
        let sys = Scratch::new();
        assert!(read(&sys.0, &CpuSet::parse("0").unwrap()).is_none());
    }

    #[test]
    fn cpu_max_rounds_up_and_max_is_no_quota() {
        assert_eq!(parse_cpu_max("max 100000\n"), None);
        assert_eq!(parse_cpu_max("150000 100000\n"), Some(2));
        assert_eq!(parse_cpu_max("200000 100000"), Some(2));
        assert_eq!(parse_cpu_max("10000 100000"), Some(1));
        assert_eq!(parse_cpu_max("garbage"), None);
    }

    #[test]
    fn the_tightest_quota_on_the_path_wins_for_v2_and_v1() {
        let root = Scratch::new();
        root.write("proc_cgroup_v2", "0::/a/b\n");
        root.write("fs/cpu.max", "max 100000\n");
        root.write("fs/a/cpu.max", "300000 100000\n");
        root.write("fs/a/b/cpu.max", "max 100000\n");
        assert_eq!(
            cgroup_quota(&root.0.join("proc_cgroup_v2"), &root.0.join("fs")),
            Some(3)
        );

        root.write("proc_cgroup_v1", "4:cpu,cpuacct:/svc\n3:memory:/svc\n");
        root.write("v1/cpu,cpuacct/cpu.cfs_quota_us", "-1\n");
        root.write("v1/cpu,cpuacct/cpu.cfs_period_us", "100000\n");
        root.write("v1/cpu,cpuacct/svc/cpu.cfs_quota_us", "50000\n");
        root.write("v1/cpu,cpuacct/svc/cpu.cfs_period_us", "100000\n");
        assert_eq!(
            cgroup_quota(&root.0.join("proc_cgroup_v1"), &root.0.join("v1")),
            Some(1)
        );
    }
}
