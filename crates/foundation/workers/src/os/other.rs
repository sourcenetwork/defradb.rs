//! Every other platform: no thread settings; the pool runs with what the OS
//! gives it and reports each setting as unsupported.

use std::io;

use super::{MemoryMode, Os, Scheduling};
use crate::config::{MemoryPolicy, QosClass, RealTimePolicy};
use crate::topology::CpuSet;

pub(super) struct System;

fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "not supported on this platform")
}

impl Os for System {
    fn thread_affinity(&self) -> io::Result<CpuSet> {
        Err(unsupported())
    }

    fn set_thread_affinity(&self, _cpus: &CpuSet) -> io::Result<()> {
        Err(unsupported())
    }

    fn current_cpu(&self) -> Option<usize> {
        None
    }

    fn set_affinity_tag(&self, _tag: usize) -> io::Result<()> {
        Err(unsupported())
    }

    fn set_realtime(&self, _policy: RealTimePolicy, _priority: u8) -> io::Result<()> {
        Err(unsupported())
    }

    fn set_nice(&self, _nice: i32) -> io::Result<()> {
        Err(unsupported())
    }

    fn nice_floor(&self) -> Option<i32> {
        None
    }

    fn set_qos(&self, _class: QosClass) -> io::Result<()> {
        Err(unsupported())
    }

    fn scheduling(&self) -> io::Result<Scheduling> {
        Err(unsupported())
    }

    fn set_memory_policy(&self, policy: &MemoryPolicy) -> io::Result<()> {
        match policy {
            MemoryPolicy::Off => Ok(()),
            _ => Err(unsupported()),
        }
    }

    fn memory_policy(&self) -> io::Result<MemoryMode> {
        Err(unsupported())
    }

    fn lock_memory(&self, _future: bool, _on_fault: bool) -> io::Result<()> {
        Err(unsupported())
    }

    fn memlock_limit(&self) -> Option<u64> {
        None
    }
}
