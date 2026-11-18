//! macOS: one node (no NUMA), cores counted per performance level by
//! `sysctl`. CPU ids are numbered here, performance cores first; macOS cannot
//! pin a thread to them, so they only give the cores identities.

use std::ffi::CString;

use super::{Core, CoreKind, CpuSet, Node, Source, Topology};

pub(super) fn detect() -> Option<Topology> {
    let levels = sysctl("hw.nperflevels").unwrap_or(0);
    let groups: Vec<(CoreKind, usize, usize)> = if levels >= 1 {
        (0..levels)
            .filter_map(|level| {
                let physical = sysctl(&format!("hw.perflevel{level}.physicalcpu"))?;
                let logical =
                    sysctl(&format!("hw.perflevel{level}.logicalcpu")).unwrap_or(physical);
                let kind = match (levels, level) {
                    (1, _) => CoreKind::Unknown,
                    (_, 0) => CoreKind::Performance,
                    _ => CoreKind::Efficiency,
                };
                Some((kind, physical, logical))
            })
            .collect()
    } else {
        let physical = sysctl("hw.physicalcpu")?;
        vec![(
            CoreKind::Unknown,
            physical,
            sysctl("hw.logicalcpu").unwrap_or(physical),
        )]
    };
    Some(Topology {
        nodes: vec![Node {
            id: 0,
            cores: number(&groups),
        }],
        source: Source::Sysctl,
        cpu_quota: None,
    })
}

/// Cores for each `(kind, physical, logical)` group, CPU ids numbered in
/// order, siblings spread evenly over the group's cores.
pub(super) fn number(groups: &[(CoreKind, usize, usize)]) -> Vec<Core> {
    let mut next = 0;
    let mut cores = Vec::new();
    for &(kind, physical, logical) in groups {
        let physical = physical.max(1);
        let per_core = (logical / physical).max(1);
        for _ in 0..physical {
            let cpus: CpuSet = (next..next + per_core).collect();
            next += per_core;
            cores.push(Core {
                package: 0,
                l3: 0,
                kind,
                cpus,
            });
        }
    }
    cores
}

fn sysctl(name: &str) -> Option<usize> {
    let name = CString::new(name).ok()?;
    let mut value: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>();
    // SAFETY: `value` is a writable c_int and `len` holds its size, as
    // sysctlbyname requires; no new value is written.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut libc::c_int).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then(|| usize::try_from(value).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_cores_come_first_and_get_their_own_ids() {
        let cores = number(&[(CoreKind::Performance, 2, 2), (CoreKind::Efficiency, 2, 2)]);
        let ids: Vec<String> = cores.iter().map(|c| c.cpus.to_string()).collect();
        assert_eq!(ids, vec!["0", "1", "2", "3"]);
        assert_eq!(cores[0].kind, CoreKind::Performance);
        assert_eq!(cores[3].kind, CoreKind::Efficiency);
    }

    #[test]
    fn hyperthreaded_intel_cores_hold_two_cpus() {
        let cores = number(&[(CoreKind::Unknown, 2, 4)]);
        let ids: Vec<String> = cores.iter().map(|c| c.cpus.to_string()).collect();
        assert_eq!(ids, vec!["0-1", "2-3"]);
    }
}
