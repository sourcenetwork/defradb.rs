//! The fallback chains against a scripted OS, so every refusal is tested
//! without privileges, plus real pinning on Linux (which needs none).

use std::cell::{Cell, RefCell};
use std::io;

use super::*;

/// An OS whose calls succeed unless named in `refused` (permission denied) or
/// `unsupported`, and which reads back what was set.
struct Fake {
    refused: Vec<&'static str>,
    unsupported: Vec<&'static str>,
    scheduling: Cell<Scheduling>,
    floor: Option<i32>,
    affinity: RefCell<CpuSet>,
    /// The mask the OS reports whatever was set (a cpuset moved under us).
    reads_back: Option<CpuSet>,
    /// The CPU the thread runs on; the mask's first when `None`.
    runs_on: Option<usize>,
    mode: Cell<MemoryMode>,
    limit: Option<u64>,
    locked: Cell<Option<(bool, bool)>>,
    tag: Cell<Option<usize>>,
}

impl Default for Fake {
    fn default() -> Self {
        Fake {
            refused: Vec::new(),
            unsupported: Vec::new(),
            scheduling: Cell::new(Scheduling::Nice(0)),
            floor: Some(0),
            affinity: RefCell::new(CpuSet::parse("0-7").unwrap()),
            reads_back: None,
            runs_on: None,
            mode: Cell::new(MemoryMode::Default),
            limit: None,
            locked: Cell::new(None),
            tag: Cell::new(None),
        }
    }
}

impl Fake {
    fn gate(&self, call: &'static str) -> io::Result<()> {
        if self.unsupported.contains(&call) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("{call} unsupported"),
            ));
        }
        if self.refused.contains(&call) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(())
    }
}

impl Os for Fake {
    fn thread_affinity(&self) -> io::Result<CpuSet> {
        self.gate("affinity")?;
        Ok(self
            .reads_back
            .clone()
            .unwrap_or_else(|| self.affinity.borrow().clone()))
    }

    fn set_thread_affinity(&self, cpus: &CpuSet) -> io::Result<()> {
        self.gate("affinity")?;
        *self.affinity.borrow_mut() = cpus.clone();
        Ok(())
    }

    fn current_cpu(&self) -> Option<usize> {
        self.runs_on.or_else(|| self.affinity.borrow().first())
    }

    fn set_affinity_tag(&self, tag: usize) -> io::Result<()> {
        self.gate("tag")?;
        self.tag.set(Some(tag));
        Ok(())
    }

    fn set_realtime(&self, policy: RealTimePolicy, priority: u8) -> io::Result<()> {
        self.gate("realtime")?;
        self.scheduling.set(Scheduling::RealTime {
            policy,
            priority: priority.into(),
        });
        Ok(())
    }

    fn set_nice(&self, nice: i32) -> io::Result<()> {
        if nice < 0 && self.refused.contains(&"nice-below-floor") && Some(nice) < self.floor {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        self.gate("nice")?;
        self.scheduling.set(Scheduling::Nice(nice));
        Ok(())
    }

    fn nice_floor(&self) -> Option<i32> {
        self.floor
    }

    fn set_qos(&self, class: QosClass) -> io::Result<()> {
        self.gate("qos")?;
        self.scheduling.set(Scheduling::Qos(Some(class)));
        Ok(())
    }

    fn scheduling(&self) -> io::Result<Scheduling> {
        Ok(self.scheduling.get())
    }

    fn set_memory_policy(&self, policy: &MemoryPolicy) -> io::Result<()> {
        match policy {
            MemoryPolicy::Off => Ok(()),
            MemoryPolicy::Local => {
                self.gate("local")?;
                self.mode.set(MemoryMode::Local);
                Ok(())
            }
            MemoryPolicy::Bind(_) => {
                self.gate("bind")?;
                self.mode.set(MemoryMode::Bind);
                Ok(())
            }
        }
    }

    fn memory_policy(&self) -> io::Result<MemoryMode> {
        Ok(self.mode.get())
    }

    fn lock_memory(&self, future: bool, on_fault: bool) -> io::Result<()> {
        if on_fault {
            self.gate("on-fault")?;
        }
        if future {
            self.gate("future")?;
        }
        self.gate("lock")?;
        self.locked.set(Some((future, on_fault)));
        Ok(())
    }

    fn memlock_limit(&self) -> Option<u64> {
        self.limit
    }
}

const RR_50: Priority = Priority::RealTime {
    policy: RealTimePolicy::RoundRobin,
    priority: 50,
};

#[test]
fn real_time_is_granted_when_allowed() {
    let os = Fake::default();
    let outcome = prioritize(&os, RR_50);
    assert_eq!(outcome.granted, "SCHED_RR 50");
    assert_eq!(outcome.fell_back, None);
}

#[test]
fn refused_real_time_falls_back_to_nice_minus_20() {
    let os = Fake {
        refused: vec!["realtime"],
        unsupported: vec!["qos"],
        ..Fake::default()
    };
    let outcome = prioritize(&os, RR_50);
    assert_eq!(outcome.asked, "SCHED_RR 50");
    assert_eq!(outcome.granted, "nice -20");
    let reasons = outcome.fell_back.unwrap();
    assert!(reasons.contains("SCHED_RR 50"), "{reasons}");
}

#[test]
fn refused_nice_minus_20_falls_back_to_the_rlimit_floor() {
    let os = Fake {
        refused: vec!["realtime", "nice-below-floor"],
        unsupported: vec!["qos"],
        floor: Some(-5),
        ..Fake::default()
    };
    assert_eq!(prioritize(&os, RR_50).granted, "nice -5");
}

#[test]
fn with_nothing_allowed_the_current_priority_stays() {
    let os = Fake {
        refused: vec!["realtime", "nice"],
        unsupported: vec!["qos"],
        scheduling: Cell::new(Scheduling::Nice(3)),
        floor: Some(3),
        ..Fake::default()
    };
    let outcome = prioritize(&os, RR_50);
    assert_eq!(outcome.granted, "nice 3");
    assert_eq!(
        outcome.fell_back.unwrap().split("; ").count(),
        4,
        "every step was tried"
    );
}

#[test]
fn macos_turns_real_time_into_user_interactive_qos() {
    let os = Fake {
        unsupported: vec!["realtime", "nice"],
        floor: None,
        ..Fake::default()
    };
    assert_eq!(prioritize(&os, RR_50).granted, "QoS UserInteractive");
}

#[test]
fn a_lower_nice_needs_no_privilege_and_inherit_changes_nothing() {
    let os = Fake {
        refused: vec!["realtime"],
        ..Fake::default()
    };
    assert_eq!(prioritize(&os, Priority::Nice(10)).granted, "nice 10");
    let outcome = prioritize(&os, Priority::Inherit);
    assert_eq!(
        (outcome.asked.as_str(), outcome.granted.as_str()),
        ("nice 10", "nice 10")
    );
}

#[test]
fn a_pin_that_reads_back_is_granted() {
    let os = Fake::default();
    let pinned = pin(
        &os,
        &CpuSet::parse("3").unwrap(),
        &CpuSet::parse("0-7").unwrap(),
        4,
    );
    assert_eq!(pinned.cpus.unwrap().to_string(), "3");
    assert_eq!(pinned.outcome.granted, "CPUs 3");
}

#[test]
fn a_pin_that_reads_back_wrong_goes_back_to_the_allowed_set() {
    let os = Fake {
        reads_back: Some(CpuSet::parse("0-1").unwrap()),
        ..Fake::default()
    };
    let pinned = pin(
        &os,
        &CpuSet::parse("3").unwrap(),
        &CpuSet::parse("0-7").unwrap(),
        4,
    );
    assert!(pinned.cpus.is_none());
    assert_eq!(pinned.outcome.granted, "unpinned");
    assert_eq!(
        os.affinity.borrow().to_string(),
        "0-7",
        "restored to the process's set"
    );
}

#[test]
fn a_thread_running_outside_its_set_fails_the_check() {
    let os = Fake {
        runs_on: Some(6),
        ..Fake::default()
    };
    let pinned = pin(
        &os,
        &CpuSet::parse("3").unwrap(),
        &CpuSet::parse("0-7").unwrap(),
        4,
    );
    assert!(pinned.outcome.fell_back.unwrap().contains("CPU 6"));
}

#[test]
fn a_refused_pin_runs_unpinned_and_an_unsupported_one_leaves_a_hint() {
    let refused = Fake {
        refused: vec!["affinity"],
        ..Fake::default()
    };
    let pinned = pin(&refused, &CpuSet::parse("3").unwrap(), &CpuSet::new(), 4);
    assert_eq!(pinned.outcome.granted, "unpinned");

    let mac = Fake {
        unsupported: vec!["affinity"],
        ..Fake::default()
    };
    let pinned = pin(&mac, &CpuSet::parse("3").unwrap(), &CpuSet::new(), 4);
    assert_eq!(pinned.outcome.granted, "unpinned, affinity tag 4 as a hint");
    assert_eq!(mac.tag.get(), Some(4));

    let silicon = Fake {
        unsupported: vec!["affinity", "tag"],
        ..Fake::default()
    };
    assert_eq!(
        pin(&silicon, &CpuSet::parse("3").unwrap(), &CpuSet::new(), 4)
            .outcome
            .granted,
        "unpinned"
    );
}

#[test]
fn bind_falls_back_to_local_and_local_to_first_touch() {
    let os = Fake {
        refused: vec!["bind"],
        ..Fake::default()
    };
    let outcome = set_memory_policy(&os, &MemoryPolicy::Bind(vec![1]));
    assert_eq!(outcome.granted, "local");
    let os = Fake {
        refused: vec!["local"],
        ..Fake::default()
    };
    assert_eq!(
        set_memory_policy(&os, &MemoryPolicy::Local).granted,
        "first touch"
    );
}

#[test]
fn future_locking_needs_a_limit_that_holds_the_footprint() {
    let lock = |footprint| MemoryLock::Lock {
        future: true,
        on_fault: true,
        footprint,
    };
    let unlimited = Fake::default();
    assert_eq!(
        lock_memory(&unlimited, &lock(None)).granted,
        "current and future, on fault"
    );

    let small = Fake {
        limit: Some(64 << 10),
        ..Fake::default()
    };
    let outcome = lock_memory(&small, &lock(Some(1 << 30)));
    assert_eq!(outcome.granted, "current, on fault");
    assert!(outcome.fell_back.unwrap().contains("RLIMIT_MEMLOCK"));
    assert_eq!(small.locked.get(), Some((false, true)));

    let unknown = Fake {
        limit: Some(1 << 40),
        ..Fake::default()
    };
    assert_eq!(
        lock_memory(&unknown, &lock(None)).granted,
        "current, on fault"
    );
    assert_eq!(
        lock_memory(&unknown, &lock(Some(1 << 30))).granted,
        "current and future, on fault"
    );
}

#[test]
fn locking_falls_back_past_on_fault_to_nothing() {
    let lock = MemoryLock::Lock {
        future: true,
        on_fault: true,
        footprint: None,
    };
    let mac = Fake {
        unsupported: vec!["on-fault"],
        ..Fake::default()
    };
    assert_eq!(lock_memory(&mac, &lock).granted, "current and future");
    let refused = Fake {
        refused: vec!["lock"],
        ..Fake::default()
    };
    assert_eq!(lock_memory(&refused, &lock).granted, "off");
    assert_eq!(lock_memory(&refused, &MemoryLock::Off).fell_back, None);
}

#[cfg(target_os = "linux")]
#[test]
fn real_pinning_takes_and_is_verified_unprivileged() {
    let allowed = current_affinity().expect("affinity readable on Linux");
    let target: CpuSet = allowed.iter().last().into_iter().collect();
    std::thread::spawn(move || {
        let pinned = pin(system(), &target, &allowed, 0);
        assert_eq!(pinned.cpus.as_ref(), Some(&target), "{:?}", pinned.outcome);
        assert_eq!(check_pin(system(), &target), Ok(()));
        assert_eq!(system().current_cpu(), target.first());
    })
    .join()
    .unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn real_priorities_read_back_and_lowering_needs_no_privilege() {
    std::thread::spawn(|| {
        let outcome = prioritize(system(), Priority::Nice(19));
        assert_eq!(outcome.granted, "nice 19");
        let outcome = prioritize(system(), RR_50);
        assert!(!outcome.granted.starts_with("unknown"), "{outcome:?}");
    })
    .join()
    .unwrap();
}
