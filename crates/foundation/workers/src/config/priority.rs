//! Thread priorities: the settings, the thread classes they apply to, and
//! each class's default per platform.

use std::fmt;

/// Linux's real-time scheduling policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealTimePolicy {
    /// `SCHED_RR`: equal-priority threads sharing a core still take turns.
    RoundRobin,
    /// `SCHED_FIFO`: a thread runs until it blocks or yields.
    Fifo,
}

/// macOS quality-of-service classes, from most to least important.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QosClass {
    /// Steers a thread to performance cores.
    UserInteractive,
    /// Work a user waits on.
    UserInitiated,
    /// The system default.
    Default,
    /// Long-running work a user does not wait on.
    Utility,
    /// Maintenance work.
    Background,
}

/// A thread's scheduling priority. Each falls back to the next weaker setting
/// the OS grants:
/// real time -> `UserInteractive` QoS (macOS) -> nice -20 -> the lowest nice
/// `RLIMIT_NICE` allows -> unchanged; a nice value -> the lowest nice allowed
/// -> unchanged; a QoS class -> unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Leave the thread as it starts.
    Inherit,
    /// A Linux real-time policy at `priority` (1 to 99). Workers park when
    /// idle, so a real-time worker never holds its core while it has nothing
    /// to run.
    RealTime {
        /// RR or FIFO.
        policy: RealTimePolicy,
        /// 1 (lowest) to 99 (highest).
        priority: u8,
    },
    /// A Linux nice value, -20 (highest) to 19 (lowest).
    Nice(i8),
    /// A macOS QoS class.
    Qos(QosClass),
}

/// The kinds of thread the pool places, each with its own priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadClass {
    /// Transaction-processing workers.
    Worker,
    /// The network edge's threads.
    Edge,
    /// Threads that run periodic jobs outside the workers.
    Periodic,
    /// regolith's compaction threads, placed through the pool's `Env`.
    Compaction,
    /// The blocking pool's threads.
    Blocking,
}

impl ThreadClass {
    /// Every class, highest priority first.
    pub const ALL: [ThreadClass; 5] = [
        ThreadClass::Worker,
        ThreadClass::Edge,
        ThreadClass::Periodic,
        ThreadClass::Compaction,
        ThreadClass::Blocking,
    ];

    /// The class's name in the report and the logs.
    pub fn name(self) -> &'static str {
        match self {
            ThreadClass::Worker => "worker",
            ThreadClass::Edge => "edge",
            ThreadClass::Periodic => "periodic",
            ThreadClass::Compaction => "compaction",
            ThreadClass::Blocking => "blocking",
        }
    }
}

/// A priority per thread class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Priorities {
    /// Workers: the highest the process may set.
    pub workers: Priority,
    /// The network edge: next.
    pub edge: Priority,
    /// Periodic jobs outside the workers: lower.
    pub periodic: Priority,
    /// regolith's compaction threads: lower.
    pub compaction: Priority,
    /// The blocking pool: lowest.
    pub blocking: Priority,
}

impl Priorities {
    /// Every class left at the priority its thread starts with.
    pub const fn inherited() -> Self {
        Priorities {
            workers: Priority::Inherit,
            edge: Priority::Inherit,
            periodic: Priority::Inherit,
            compaction: Priority::Inherit,
            blocking: Priority::Inherit,
        }
    }

    /// The priority for `class`.
    pub fn of(&self, class: ThreadClass) -> Priority {
        match class {
            ThreadClass::Worker => self.workers,
            ThreadClass::Edge => self.edge,
            ThreadClass::Periodic => self.periodic,
            ThreadClass::Compaction => self.compaction,
            ThreadClass::Blocking => self.blocking,
        }
    }
}

impl Default for Priorities {
    #[cfg(target_os = "linux")]
    fn default() -> Self {
        Priorities {
            workers: Priority::RealTime {
                policy: RealTimePolicy::RoundRobin,
                priority: 50,
            },
            edge: Priority::Nice(-10),
            periodic: Priority::Nice(5),
            compaction: Priority::Nice(10),
            blocking: Priority::Nice(19),
        }
    }

    #[cfg(target_os = "macos")]
    fn default() -> Self {
        Priorities {
            workers: Priority::Qos(QosClass::UserInteractive),
            edge: Priority::Qos(QosClass::UserInitiated),
            periodic: Priority::Qos(QosClass::Utility),
            compaction: Priority::Qos(QosClass::Utility),
            blocking: Priority::Qos(QosClass::Background),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn default() -> Self {
        Priorities::inherited()
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Inherit => f.write_str("inherited"),
            Priority::RealTime {
                policy: RealTimePolicy::RoundRobin,
                priority,
            } => {
                write!(f, "SCHED_RR {priority}")
            }
            Priority::RealTime {
                policy: RealTimePolicy::Fifo,
                priority,
            } => {
                write!(f, "SCHED_FIFO {priority}")
            }
            Priority::Nice(nice) => write!(f, "nice {nice}"),
            Priority::Qos(class) => write!(f, "QoS {class:?}"),
        }
    }
}
