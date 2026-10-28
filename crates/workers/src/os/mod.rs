//! Thread and process settings: pinning, the NUMA memory policy, memory
//! locking and priorities.
//!
//! The primitive calls sit behind the small [`Os`] trait, one implementation
//! per platform, so the fallback chains below are written once and tested
//! without privileges against a scripted fake. No setting is ever a hard
//! error: each chain tries the setting asked for, then the next weaker one,
//! and returns an [`Outcome`] naming what was asked, what the OS granted and
//! why it fell back.

use std::fmt;
use std::io;

use crate::config::{MemoryLock, MemoryPolicy, Priority, QosClass, RealTimePolicy};
use crate::topology::CpuSet;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod other;
#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
use linux::System;
#[cfg(target_os = "macos")]
use macos::System;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use other::System;

/// The primitive OS calls, each on the calling thread. A platform without a
/// call returns [`io::ErrorKind::Unsupported`].
pub(crate) trait Os {
    /// The calling thread's CPU mask.
    fn thread_affinity(&self) -> io::Result<CpuSet>;
    /// Restricts the calling thread to `cpus`.
    fn set_thread_affinity(&self, cpus: &CpuSet) -> io::Result<()>;
    /// The CPU the calling thread runs on now.
    fn current_cpu(&self) -> Option<usize>;
    /// A placement hint where threads cannot be pinned (Intel Macs).
    fn set_affinity_tag(&self, tag: usize) -> io::Result<()>;
    /// A real-time policy for the calling thread.
    fn set_realtime(&self, policy: RealTimePolicy, priority: u8) -> io::Result<()>;
    /// The calling thread's nice value.
    fn set_nice(&self, nice: i32) -> io::Result<()>;
    /// The lowest nice value the process may set unprivileged (`RLIMIT_NICE`).
    fn nice_floor(&self) -> Option<i32>;
    /// The calling thread's QoS class.
    fn set_qos(&self, class: QosClass) -> io::Result<()>;
    /// How the calling thread is scheduled now.
    fn scheduling(&self) -> io::Result<Scheduling>;
    /// The calling thread's NUMA memory policy.
    fn set_memory_policy(&self, policy: &MemoryPolicy) -> io::Result<()>;
    /// The calling thread's memory policy now.
    fn memory_policy(&self) -> io::Result<MemoryMode>;
    /// `mlockall` for the whole process.
    fn lock_memory(&self, future: bool, on_fault: bool) -> io::Result<()>;
    /// `RLIMIT_MEMLOCK` in bytes; `None` when unlimited.
    fn memlock_limit(&self) -> Option<u64>;
    /// The calling thread's id, by which another thread reads its counters.
    fn thread_id(&self) -> Option<u64> {
        None
    }
    /// How often thread `tid` of this process was switched out so far.
    fn context_switches(&self, _tid: u64) -> io::Result<ContextSwitches> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "per-thread context switches are not readable on this platform",
        ))
    }
}

/// How often a thread was switched out of its CPU: because it waited
/// (`voluntary`: a park, a blocking call) or because the scheduler took the
/// CPU from it (`involuntary`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContextSwitches {
    /// The thread gave up its CPU to wait.
    pub voluntary: u64,
    /// The scheduler took its CPU away.
    pub involuntary: u64,
}

/// The running platform's calls.
pub(crate) fn system() -> &'static dyn Os {
    static SYSTEM: System = System;
    &SYSTEM
}

/// The calling thread's CPU mask, where the platform says.
pub fn current_affinity() -> Option<CpuSet> {
    system().thread_affinity().ok()
}

/// How a thread is scheduled, as read back from the OS. Each platform builds
/// only its own variants.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scheduling {
    RealTime {
        policy: RealTimePolicy,
        priority: i32,
    },
    Nice(i32),
    Qos(Option<QosClass>),
}

impl fmt::Display for Scheduling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scheduling::RealTime {
                policy: RealTimePolicy::RoundRobin,
                priority,
            } => {
                write!(f, "SCHED_RR {priority}")
            }
            Scheduling::RealTime {
                policy: RealTimePolicy::Fifo,
                priority,
            } => {
                write!(f, "SCHED_FIFO {priority}")
            }
            Scheduling::Nice(nice) => write!(f, "nice {nice}"),
            Scheduling::Qos(Some(class)) => write!(f, "QoS {class:?}"),
            Scheduling::Qos(None) => f.write_str("QoS unspecified"),
        }
    }
}

/// A thread's NUMA memory policy, as read back from the OS. Only Linux
/// builds the NUMA variants.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoryMode {
    Default,
    Local,
    Bind,
    Other(i32),
}

impl fmt::Display for MemoryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryMode::Default => f.write_str("first touch"),
            MemoryMode::Local => f.write_str("local"),
            MemoryMode::Bind => f.write_str("bind"),
            MemoryMode::Other(mode) => write!(f, "mode {mode}"),
        }
    }
}

/// What one setting asked for, what the OS granted, and why it fell back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The setting asked for.
    pub asked: String,
    /// What is in effect.
    pub granted: String,
    /// Each refusal on the way down the fallback chain, when there was one.
    pub fell_back: Option<String>,
}

impl Outcome {
    fn new(asked: impl fmt::Display, granted: impl fmt::Display, reasons: Vec<String>) -> Outcome {
        Outcome {
            asked: asked.to_string(),
            granted: granted.to_string(),
            fell_back: (!reasons.is_empty()).then(|| reasons.join("; ")),
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.asked == self.granted {
            return f.write_str(&self.granted);
        }
        write!(f, "{} (asked {})", self.granted, self.asked)
    }
}

/// Pinning's result: the outcome, and the set the thread is verified to be
/// pinned to.
#[derive(Clone, Debug)]
pub(crate) struct Pinned {
    pub outcome: Outcome,
    pub cpus: Option<CpuSet>,
}

/// Pins the calling thread to `cpus` and verifies it: the mask reads back as
/// `cpus` and the thread runs on one of them. On a refusal or a failed check
/// the thread goes back to `allowed` (the process's set) and runs unpinned;
/// where pinning is unsupported, `tag` is set as an affinity hint if the
/// platform takes one. An empty `cpus` asks for no pinning.
pub(crate) fn pin(os: &dyn Os, cpus: &CpuSet, allowed: &CpuSet, tag: usize) -> Pinned {
    if cpus.is_empty() {
        return Pinned {
            outcome: Outcome::new("unpinned", "unpinned", Vec::new()),
            cpus: None,
        };
    }
    let asked = format!("CPUs {cpus}");
    let reason = match os.set_thread_affinity(cpus) {
        Ok(()) => match check_pin(os, cpus) {
            Ok(()) => {
                return Pinned {
                    outcome: Outcome::new(&asked, &asked, Vec::new()),
                    cpus: Some(cpus.clone()),
                };
            }
            Err(reason) => {
                if !allowed.is_empty() {
                    let _ = os.set_thread_affinity(allowed);
                }
                reason
            }
        },
        Err(e) if e.kind() == io::ErrorKind::Unsupported => match os.set_affinity_tag(tag) {
            Ok(()) => {
                let granted = format!("unpinned, affinity tag {tag} as a hint");
                return Pinned {
                    outcome: Outcome::new(asked, granted, vec![e.to_string()]),
                    cpus: None,
                };
            }
            Err(_) => e.to_string(),
        },
        Err(e) => e.to_string(),
    };
    Pinned {
        outcome: Outcome::new(asked, "unpinned", vec![reason]),
        cpus: None,
    }
}

/// Whether the calling thread is pinned to exactly `cpus` and runs on one of
/// them; the reason when not.
pub(crate) fn check_pin(os: &dyn Os, cpus: &CpuSet) -> Result<(), String> {
    let mask = os
        .thread_affinity()
        .map_err(|e| format!("the CPU mask cannot be read back: {e}"))?;
    if &mask != cpus {
        return Err(format!("the CPU mask reads back as {mask}"));
    }
    match os.current_cpu() {
        Some(cpu) if !cpus.contains(cpu) => Err(format!("the thread runs on CPU {cpu}")),
        _ => Ok(()),
    }
}

/// One attempt in a priority fallback chain.
#[derive(Clone, Copy, Debug)]
enum Step {
    RealTime(RealTimePolicy, u8),
    Qos(QosClass),
    Nice(i32),
    /// The lowest nice `RLIMIT_NICE` allows, no lower than the value given.
    NiceFloor(i32),
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::RealTime(policy, priority) => fmt::Display::fmt(
                &Priority::RealTime {
                    policy: *policy,
                    priority: *priority,
                },
                f,
            ),
            Step::Qos(class) => write!(f, "QoS {class:?}"),
            Step::Nice(nice) => write!(f, "nice {nice}"),
            Step::NiceFloor(_) => f.write_str("the lowest nice RLIMIT_NICE allows"),
        }
    }
}

/// The chain for `wanted`, strongest first. Each platform refuses the steps
/// it has no call for, so one chain serves Linux and macOS.
fn chain(wanted: Priority) -> Vec<Step> {
    match wanted {
        Priority::Inherit => Vec::new(),
        Priority::RealTime { policy, priority } => vec![
            Step::RealTime(policy, priority),
            Step::Qos(QosClass::UserInteractive),
            Step::Nice(-20),
            Step::NiceFloor(-20),
        ],
        Priority::Nice(nice) if nice < 0 => {
            vec![Step::Nice(nice.into()), Step::NiceFloor(nice.into())]
        }
        Priority::Nice(nice) => vec![Step::Nice(nice.into())],
        Priority::Qos(class) => vec![Step::Qos(class)],
    }
}

fn attempt(os: &dyn Os, step: Step) -> io::Result<()> {
    match step {
        Step::RealTime(policy, priority) => os.set_realtime(policy, priority),
        Step::Qos(class) => os.set_qos(class),
        Step::Nice(nice) => os.set_nice(nice),
        Step::NiceFloor(lowest) => {
            let floor = os.nice_floor().ok_or_else(|| {
                io::Error::new(io::ErrorKind::Unsupported, "no nice limit to read")
            })?;
            let current = match os.scheduling()? {
                Scheduling::Nice(current) => current,
                other => return Err(io::Error::other(format!("the thread runs at {other}"))),
            };
            let target = floor.max(lowest);
            if target >= current {
                return Err(io::Error::other(format!(
                    "the process may not go below nice {current}"
                )));
            }
            os.set_nice(target)
        }
    }
}

/// Sets the calling thread's priority, falling back down `wanted`'s chain,
/// and reads back what is in effect.
pub(crate) fn prioritize(os: &dyn Os, wanted: Priority) -> Outcome {
    let mut reasons = Vec::new();
    for step in chain(wanted) {
        match attempt(os, step) {
            Ok(()) => break,
            Err(e) => reasons.push(format!("{step}: {e}")),
        }
    }
    let granted = match os.scheduling() {
        Ok(now) => now.to_string(),
        Err(_) if wanted == Priority::Inherit => "inherited".to_owned(),
        Err(e) => format!("unknown ({e})"),
    };
    let asked = if wanted == Priority::Inherit {
        granted.clone()
    } else {
        wanted.to_string()
    };
    Outcome::new(asked, granted, reasons)
}

/// Sets the calling thread's NUMA memory policy: bind falls back to local,
/// local to the process's own (first touch).
pub(crate) fn set_memory_policy(os: &dyn Os, wanted: &MemoryPolicy) -> Outcome {
    const LOCAL: MemoryPolicy = MemoryPolicy::Local;
    let steps = match wanted {
        MemoryPolicy::Off => vec![],
        MemoryPolicy::Local => vec![&LOCAL],
        MemoryPolicy::Bind(_) => vec![wanted, &LOCAL],
    };
    let mut reasons = Vec::new();
    for step in steps {
        match os.set_memory_policy(step) {
            Ok(()) => break,
            Err(e) => reasons.push(format!("{}: {e}", policy_name(step))),
        }
    }
    let granted = match os.memory_policy() {
        Ok(mode) => mode.to_string(),
        Err(_) => MemoryMode::Default.to_string(),
    };
    Outcome::new(policy_name(wanted), granted, reasons)
}

fn policy_name(policy: &MemoryPolicy) -> String {
    match policy {
        MemoryPolicy::Off => MemoryMode::Default.to_string(),
        MemoryPolicy::Local => MemoryMode::Local.to_string(),
        MemoryPolicy::Bind(nodes) => format!("bind to nodes {nodes:?}"),
    }
}

/// Locks the process's memory, falling back from on-fault to populated, from
/// future mappings to current ones, and then to nothing. Future mappings are
/// locked only when `RLIMIT_MEMLOCK` is unlimited or holds the footprint,
/// since past the limit every later allocation fails.
pub(crate) fn lock_memory(os: &dyn Os, wanted: &MemoryLock) -> Outcome {
    let MemoryLock::Lock {
        future,
        on_fault,
        footprint,
    } = *wanted
    else {
        return Outcome::new("off", "off", Vec::new());
    };
    let mut reasons = Vec::new();
    let limit = os.memlock_limit();
    let fits = match (limit, footprint) {
        (None, _) => true,
        (Some(limit), Some(footprint)) => footprint <= limit,
        (Some(_), None) => false,
    };
    let mut steps = Vec::with_capacity(4);
    for step in [
        (future, on_fault),
        (future, false),
        (false, on_fault),
        (false, false),
    ] {
        if !steps.contains(&step) {
            steps.push(step);
        }
    }
    let mut granted = "off".to_owned();
    for (future, on_fault) in steps {
        if future && !fits {
            reasons.push(match (limit, footprint) {
                (Some(limit), Some(footprint)) => format!(
                    "{}: RLIMIT_MEMLOCK {limit} bytes is below the footprint {footprint}",
                    lock_name(true, on_fault)
                ),
                _ => format!(
                    "{}: no footprint set to check RLIMIT_MEMLOCK against",
                    lock_name(true, on_fault)
                ),
            });
            continue;
        }
        match os.lock_memory(future, on_fault) {
            Ok(()) => {
                granted = lock_name(future, on_fault);
                break;
            }
            Err(e) => reasons.push(format!("{}: {e}", lock_name(future, on_fault))),
        }
    }
    Outcome::new(lock_name(future, on_fault), granted, reasons)
}

fn lock_name(future: bool, on_fault: bool) -> String {
    let what = if future {
        "current and future"
    } else {
        "current"
    };
    if on_fault {
        format!("{what}, on fault")
    } else {
        what.to_owned()
    }
}
