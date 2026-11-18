//! macOS: no pinning (Apple Silicon reports no affinity sets; Intel Macs take
//! an affinity tag as a placement hint), priorities through QoS classes only
//! (`pthread_setschedparam` would take the thread out of the QoS system for
//! good, so it is never called), one memory system, and `mlockall` without
//! on-fault locking.

use std::io;

use super::{MemoryMode, Os, Scheduling};
use crate::config::{MemoryPolicy, QosClass, RealTimePolicy};
use crate::topology::CpuSet;

pub(super) struct System;

fn unsupported(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, what)
}

fn qos(class: QosClass) -> libc::qos_class_t {
    match class {
        QosClass::UserInteractive => libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE,
        QosClass::UserInitiated => libc::qos_class_t::QOS_CLASS_USER_INITIATED,
        QosClass::Default => libc::qos_class_t::QOS_CLASS_DEFAULT,
        QosClass::Utility => libc::qos_class_t::QOS_CLASS_UTILITY,
        QosClass::Background => libc::qos_class_t::QOS_CLASS_BACKGROUND,
    }
}

impl Os for System {
    fn thread_affinity(&self) -> io::Result<CpuSet> {
        Err(unsupported("macOS cannot pin threads"))
    }

    fn set_thread_affinity(&self, _cpus: &CpuSet) -> io::Result<()> {
        Err(unsupported("macOS cannot pin threads"))
    }

    fn current_cpu(&self) -> Option<usize> {
        None
    }

    fn set_affinity_tag(&self, tag: usize) -> io::Result<()> {
        let mut policy = libc::thread_affinity_policy {
            affinity_tag: libc::integer_t::try_from(tag).unwrap_or(libc::integer_t::MAX),
        };
        // SAFETY: pthread_mach_thread_np names the calling thread without
        // taking a port reference; `policy` is the affinity policy and the
        // count is its size in integers.
        let result = unsafe {
            libc::thread_policy_set(
                libc::pthread_mach_thread_np(libc::pthread_self()),
                libc::THREAD_AFFINITY_POLICY as libc::thread_policy_flavor_t,
                (&mut policy as *mut libc::thread_affinity_policy).cast(),
                libc::THREAD_AFFINITY_POLICY_COUNT,
            )
        };
        match result {
            0 => Ok(()),
            libc::KERN_NOT_SUPPORTED => Err(unsupported("this Mac takes no affinity hints")),
            other => Err(io::Error::other(format!(
                "thread_policy_set returned {other}"
            ))),
        }
    }

    fn set_realtime(&self, _policy: RealTimePolicy, _priority: u8) -> io::Result<()> {
        Err(unsupported(
            "macOS schedules general work by QoS class, not real-time policy",
        ))
    }

    fn set_nice(&self, _nice: i32) -> io::Result<()> {
        Err(unsupported("macOS sets a thread's priority by QoS class"))
    }

    fn nice_floor(&self) -> Option<i32> {
        None
    }

    fn set_qos(&self, class: QosClass) -> io::Result<()> {
        // SAFETY: sets the calling thread's own QoS class.
        match unsafe { libc::pthread_set_qos_class_self_np(qos(class), 0) } {
            0 => Ok(()),
            errno => Err(io::Error::from_raw_os_error(errno)),
        }
    }

    fn scheduling(&self) -> io::Result<Scheduling> {
        let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
        let mut relative: libc::c_int = 0;
        // SAFETY: both outputs are writable; the thread is the caller.
        let rc = unsafe {
            libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut relative)
        };
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        let all = [
            QosClass::UserInteractive,
            QosClass::UserInitiated,
            QosClass::Default,
            QosClass::Utility,
            QosClass::Background,
        ];
        Ok(Scheduling::Qos(
            all.into_iter().find(|c| qos(*c) as u32 == class as u32),
        ))
    }

    fn set_memory_policy(&self, policy: &MemoryPolicy) -> io::Result<()> {
        match policy {
            MemoryPolicy::Off => Ok(()),
            _ => Err(unsupported("macOS has one memory system")),
        }
    }

    fn memory_policy(&self) -> io::Result<MemoryMode> {
        Err(unsupported("macOS has one memory system"))
    }

    fn lock_memory(&self, future: bool, on_fault: bool) -> io::Result<()> {
        if on_fault {
            return Err(unsupported("macOS has no on-fault locking"));
        }
        let flags = if future {
            libc::MCL_CURRENT | libc::MCL_FUTURE
        } else {
            libc::MCL_CURRENT
        };
        // SAFETY: mlockall takes only flags.
        match unsafe { libc::mlockall(flags) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    fn memlock_limit(&self) -> Option<u64> {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: `limit` is a valid rlimit to write.
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut limit) };
        (rc == 0 && limit.rlim_cur != libc::RLIM_INFINITY).then_some(limit.rlim_cur)
    }
}
