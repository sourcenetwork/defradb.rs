//! What the pool asked the OS for and what it was granted, logged once at
//! start (info), with one warning per setting that fell back.

use std::fmt;

use super::placer::ThreadPlacement;
use crate::os::Outcome;
use crate::topology::{Source, Topology};
use crate::WorkerId;

/// The machine as the pool found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologySummary {
    /// NUMA nodes with an allowed CPU.
    pub nodes: usize,
    /// Packages with an allowed CPU.
    pub packages: usize,
    /// L3 domains with an allowed CPU.
    pub l3_domains: usize,
    /// Allowed physical cores.
    pub cores: usize,
    /// Allowed logical CPUs.
    pub cpus: usize,
    /// The cgroup CPU quota, in whole CPUs.
    pub cpu_quota: Option<usize>,
    /// Where the topology was read from.
    pub source: Source,
}

impl TopologySummary {
    pub(crate) fn of(topology: &Topology) -> TopologySummary {
        TopologySummary {
            nodes: topology.nodes.len(),
            packages: topology.package_count(),
            l3_domains: topology.l3_count(),
            cores: topology.core_count(),
            cpus: topology.cpu_count(),
            cpu_quota: topology.cpu_quota,
            source: topology.source,
        }
    }
}

/// One worker's placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerReport {
    /// The worker.
    pub id: WorkerId,
    /// What placing it came to.
    pub placement: ThreadPlacement,
}

/// The pool's start-up report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// The machine.
    pub topology: TopologySummary,
    /// Each worker's placement.
    pub workers: Vec<WorkerReport>,
    /// Locking the process's memory.
    pub memory_lock: Outcome,
    /// Things the plan had to say (oversubscription, unusable custom sets).
    pub notes: Vec<String>,
}

/// "asked: granted on k/n, other on m/n", counting how many threads got what.
fn tally<'a, I>(outcomes: I) -> (String, Option<String>)
where
    I: Iterator<Item = &'a Outcome>,
{
    let outcomes: Vec<&Outcome> = outcomes.collect();
    let total = outcomes.len();
    let mut groups: Vec<(String, usize)> = Vec::new();
    for outcome in &outcomes {
        let granted = if outcome.fell_back.is_none() && outcome.granted.starts_with("CPUs") {
            "pinned".to_owned()
        } else {
            outcome.granted.clone()
        };
        match groups.iter_mut().find(|(g, _)| *g == granted) {
            Some((_, count)) => *count += 1,
            None => groups.push((granted, 1)),
        }
    }
    let asked = outcomes.first().map_or("", |o| o.asked.as_str());
    let asked = if asked.starts_with("CPUs") {
        "pinned"
    } else {
        asked
    };
    let granted: Vec<String> = groups
        .iter()
        .map(|(g, n)| format!("{g} on {n}/{total}"))
        .collect();
    let reason = outcomes.iter().find_map(|o| o.fell_back.clone());
    (format!("{asked}: {}", granted.join(", ")), reason)
}

impl Report {
    fn settings(&self) -> [(&'static str, String, Option<String>); 3] {
        let workers = &self.workers;
        let pinning = tally(workers.iter().map(|w| &w.placement.pinning));
        let memory = tally(workers.iter().map(|w| &w.placement.memory_policy));
        let priority = tally(workers.iter().map(|w| &w.placement.priority));
        [
            ("pinning", pinning.0, pinning.1),
            ("memory policy", memory.0, memory.1),
            ("priority", priority.0, priority.1),
        ]
    }

    /// Logs the report: one info line, and one warning per setting that fell
    /// back on any worker.
    pub(crate) fn log(&self) {
        let [pinning, memory, priority] = self.settings();
        tracing::info!(
            workers = self.workers.len(),
            nodes = self.topology.nodes,
            cores = self.topology.cores,
            cpus = self.topology.cpus,
            cpu_quota = ?self.topology.cpu_quota,
            pinning = pinning.1.as_str(),
            memory_policy = memory.1.as_str(),
            priority = priority.1.as_str(),
            memory_lock = %self.memory_lock,
            "worker pool started"
        );
        for (setting, granted, reason) in [pinning, memory, priority] {
            if let Some(reason) = reason {
                tracing::warn!(
                    setting,
                    granted = granted.as_str(),
                    reason = reason.as_str(),
                    "a worker setting fell back to a weaker one"
                );
            }
        }
        if let Some(reason) = &self.memory_lock.fell_back {
            tracing::warn!(granted = %self.memory_lock, reason = reason.as_str(), "memory locking fell back to a weaker one");
        }
        for note in &self.notes {
            tracing::warn!(note = note.as_str(), "worker placement");
        }
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [pinning, memory, priority] = self.settings();
        write!(
            f,
            "{} workers on {} node(s), {} cores, {} CPUs; {} {}; {} {}; {} {}; memory lock {}",
            self.workers.len(),
            self.topology.nodes,
            self.topology.cores,
            self.topology.cpus,
            pinning.0,
            pinning.1,
            memory.0,
            memory.1,
            priority.0,
            priority.1,
            self.memory_lock
        )?;
        self.notes.iter().try_for_each(|note| write!(f, "; {note}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(asked: &str, granted: &str, fell_back: Option<&str>) -> Outcome {
        Outcome {
            asked: asked.into(),
            granted: granted.into(),
            fell_back: fell_back.map(Into::into),
        }
    }

    #[test]
    fn tally_counts_what_each_thread_was_granted() {
        let outcomes = [
            outcome("SCHED_RR 50", "nice -20", Some("SCHED_RR 50: denied")),
            outcome("SCHED_RR 50", "nice -20", Some("SCHED_RR 50: denied")),
            outcome("SCHED_RR 50", "SCHED_RR 50", None),
        ];
        let (line, reason) = tally(outcomes.iter());
        assert_eq!(line, "SCHED_RR 50: nice -20 on 2/3, SCHED_RR 50 on 1/3");
        assert_eq!(reason.as_deref(), Some("SCHED_RR 50: denied"));

        let pins = [
            outcome("CPUs 0", "CPUs 0", None),
            outcome("CPUs 1", "unpinned", Some("EPERM")),
        ];
        assert_eq!(
            tally(pins.iter()).0,
            "pinned: pinned on 1/2, unpinned on 1/2"
        );
    }
}
