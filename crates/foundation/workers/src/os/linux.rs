//! Linux: `sched_setaffinity`, `sched_setscheduler`, `setpriority` on the
//! thread id, `set_mempolicy` through `syscall`, and `mlockall`. Every call acts
//! on the calling thread (`pid` 0 or `gettid`).

use std::io;
use std::mem::size_of;

use super::{ContextSwitches, MemoryMode, Os, Scheduling};
use crate::config::{MemoryPolicy, QosClass, RealTimePolicy};
use crate::topology::{CpuSet, MAX_CPUS};

type Word = libc::c_ulong;
const WORD_BITS: usize = Word::BITS as usize;

const MPOL_DEFAULT: libc::c_int = 0;
const MPOL_BIND: libc::c_int = 2;
const MPOL_LOCAL: libc::c_int = 4;
/// Node ids above this are refused rather than sized into a mask.
const MAX_NODES: usize = 4096;

pub(super) struct System;

fn check(rc: libc::c_int) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn check_long(rc: libc::c_long) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// A kernel bitmask (an array of `unsigned long`) holding `ids`.
fn mask<I>(ids: I, bits: usize) -> Vec<Word>
where
    I: IntoIterator<Item = usize>,
{
    let mut words = vec![0 as Word; bits.div_ceil(WORD_BITS).max(1)];
    for id in ids.into_iter().filter(|id| *id < bits) {
        words[id / WORD_BITS] |= 1 << (id % WORD_BITS);
    }
    words
}

fn tid() -> libc::id_t {
    // SAFETY: gettid has no preconditions.
    unsafe { libc::gettid() as libc::id_t }
}

fn rlimit(resource: Resource) -> Option<libc::rlimit> {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is a valid rlimit to write.
    let rc = unsafe { libc::getrlimit(resource.0, &mut limit) };
    (rc == 0).then_some(limit)
}

#[cfg(target_env = "gnu")]
struct Resource(libc::__rlimit_resource_t);
#[cfg(not(target_env = "gnu"))]
struct Resource(libc::c_int);

/// The `voluntary_ctxt_switches` and `nonvoluntary_ctxt_switches` lines of a
/// `/proc/<pid>/task/<tid>/status` file.
pub(super) fn parse_switches(status: &str) -> io::Result<ContextSwitches> {
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().parse::<u64>().ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("no {name} in the thread status"),
                )
            })
    };
    Ok(ContextSwitches {
        voluntary: field("voluntary_ctxt_switches:")?,
        involuntary: field("nonvoluntary_ctxt_switches:")?,
    })
}

impl Os for System {
    fn thread_id(&self) -> Option<u64> {
        Some(u64::from(tid()))
    }

    fn context_switches(&self, tid: u64) -> io::Result<ContextSwitches> {
        parse_switches(&std::fs::read_to_string(format!(
            "/proc/self/task/{tid}/status"
        ))?)
    }

    fn thread_affinity(&self) -> io::Result<CpuSet> {
        let mut words = vec![0 as Word; MAX_CPUS / WORD_BITS];
        // SAFETY: the buffer holds `len` bytes, which is what the kernel may
        // write; pid 0 is the calling thread.
        let rc = unsafe {
            libc::sched_getaffinity(
                0,
                words.len() * size_of::<Word>(),
                words.as_mut_ptr().cast(),
            )
        };
        check(rc)?;
        Ok(words
            .iter()
            .enumerate()
            .flat_map(|(index, word)| {
                (0..WORD_BITS)
                    .filter(move |bit| word & (1 << bit) != 0)
                    .map(move |bit| index * WORD_BITS + bit)
            })
            .collect())
    }

    fn set_thread_affinity(&self, cpus: &CpuSet) -> io::Result<()> {
        let words = mask(cpus.iter(), MAX_CPUS);
        // SAFETY: the mask holds `len` bytes; pid 0 is the calling thread.
        let rc = unsafe {
            libc::sched_setaffinity(0, words.len() * size_of::<Word>(), words.as_ptr().cast())
        };
        check(rc)
    }

    fn current_cpu(&self) -> Option<usize> {
        // SAFETY: sched_getcpu has no preconditions.
        usize::try_from(unsafe { libc::sched_getcpu() }).ok()
    }

    fn set_affinity_tag(&self, _tag: usize) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Linux pins threads instead",
        ))
    }

    fn set_realtime(&self, policy: RealTimePolicy, priority: u8) -> io::Result<()> {
        let policy = match policy {
            RealTimePolicy::RoundRobin => libc::SCHED_RR,
            RealTimePolicy::Fifo => libc::SCHED_FIFO,
        };
        // SAFETY: plain queries with no preconditions.
        let (low, high) = unsafe {
            (
                libc::sched_get_priority_min(policy),
                libc::sched_get_priority_max(policy),
            )
        };
        // SAFETY: an all-zero sched_param is valid; only the priority is set.
        let mut param: libc::sched_param = unsafe { std::mem::zeroed() };
        param.sched_priority = libc::c_int::from(priority).clamp(low, high);
        // SAFETY: `param` is a valid sched_param; pid 0 is the calling thread.
        check(unsafe { libc::sched_setscheduler(0, policy, &param) })
    }

    fn set_nice(&self, nice: i32) -> io::Result<()> {
        // SAFETY: setpriority on our own thread id.
        check(unsafe { libc::setpriority(libc::PRIO_PROCESS, tid(), nice) })
    }

    fn nice_floor(&self) -> Option<i32> {
        let limit = rlimit(Resource(libc::RLIMIT_NICE))?;
        if limit.rlim_cur == libc::RLIM_INFINITY {
            return Some(-20);
        }
        // RLIMIT_NICE n lets a thread go down to nice 20 - n.
        Some(20 - i32::try_from(limit.rlim_cur.min(40)).unwrap_or(40))
    }

    fn set_qos(&self, _class: QosClass) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "QoS classes exist only on macOS",
        ))
    }

    fn scheduling(&self) -> io::Result<Scheduling> {
        // SAFETY: pid 0 is the calling thread.
        let policy = unsafe { libc::sched_getscheduler(0) };
        if policy < 0 {
            return Err(io::Error::last_os_error());
        }
        let policy = policy & !libc::SCHED_RESET_ON_FORK;
        if policy == libc::SCHED_RR || policy == libc::SCHED_FIFO {
            // SAFETY: an all-zero sched_param is valid to be written over.
            let mut param: libc::sched_param = unsafe { std::mem::zeroed() };
            // SAFETY: `param` is writable; pid 0 is the calling thread.
            check(unsafe { libc::sched_getparam(0, &mut param) })?;
            let policy = if policy == libc::SCHED_RR {
                RealTimePolicy::RoundRobin
            } else {
                RealTimePolicy::Fifo
            };
            return Ok(Scheduling::RealTime {
                policy,
                priority: param.sched_priority,
            });
        }
        // getpriority returns -1 for nice -1 as well as for an error, so errno
        // is cleared first and read after.
        // SAFETY: __errno_location returns this thread's errno.
        unsafe { *libc::__errno_location() = 0 };
        // SAFETY: getpriority on our own thread id.
        let nice = unsafe { libc::getpriority(libc::PRIO_PROCESS, tid()) };
        let error = io::Error::last_os_error();
        if nice == -1 && error.raw_os_error() != Some(0) {
            return Err(error);
        }
        Ok(Scheduling::Nice(nice))
    }

    fn set_memory_policy(&self, policy: &MemoryPolicy) -> io::Result<()> {
        let (mode, nodes) = match policy {
            MemoryPolicy::Off => return Ok(()),
            MemoryPolicy::Local => (MPOL_LOCAL, Vec::new()),
            MemoryPolicy::Bind(nodes) => {
                if nodes.is_empty() || nodes.iter().any(|n| *n >= MAX_NODES) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "node ids must be listed and below 4096",
                    ));
                }
                (MPOL_BIND, nodes.clone())
            }
        };
        let words = mask(
            nodes.iter().copied(),
            nodes.iter().max().map_or(0, |n| n + 1),
        );
        // The kernel reads one bit fewer than `maxnode` says.
        let max_node = (words.len() * WORD_BITS + 1) as libc::c_ulong;
        let (pointer, max_node) = if mode == MPOL_LOCAL {
            (std::ptr::null(), 0)
        } else {
            (words.as_ptr(), max_node)
        };
        // SAFETY: the mask holds `max_node - 1` bits, or is null with 0 for
        // MPOL_LOCAL; the policy applies to the calling thread.
        check_long(unsafe { libc::syscall(libc::SYS_set_mempolicy, mode, pointer, max_node) })
    }

    fn memory_policy(&self) -> io::Result<MemoryMode> {
        let mut mode: libc::c_int = 0;
        // SAFETY: only the mode is asked for; the mask pointer is null with a
        // zero length, as get_mempolicy allows.
        check_long(unsafe {
            libc::syscall(
                libc::SYS_get_mempolicy,
                &mut mode as *mut libc::c_int,
                std::ptr::null_mut::<Word>(),
                0 as libc::c_ulong,
                std::ptr::null_mut::<libc::c_void>(),
                0 as libc::c_ulong,
            )
        })?;
        // The high bits carry mode flags (static or relative nodes).
        Ok(match mode & 0xff {
            MPOL_DEFAULT => MemoryMode::Default,
            MPOL_LOCAL => MemoryMode::Local,
            MPOL_BIND => MemoryMode::Bind,
            other => MemoryMode::Other(other),
        })
    }

    fn lock_memory(&self, future: bool, on_fault: bool) -> io::Result<()> {
        let mut flags = libc::MCL_CURRENT;
        if future {
            flags |= libc::MCL_FUTURE;
        }
        if on_fault {
            flags |= libc::MCL_ONFAULT;
        }
        // SAFETY: mlockall takes only flags.
        check(unsafe { libc::mlockall(flags) })
    }

    fn memlock_limit(&self) -> Option<u64> {
        let limit = rlimit(Resource(libc::RLIMIT_MEMLOCK))?;
        // `rlim_t` is narrower than u64 on some 32-bit targets.
        #[allow(clippy::unnecessary_cast)]
        (limit.rlim_cur != libc::RLIM_INFINITY).then_some(limit.rlim_cur as u64)
    }
}
