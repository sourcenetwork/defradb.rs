//! The `workers` section: the worker pool the node's data path runs on.
//!
//! Every setting left unset keeps the pool's own default
//! (`workers::Config::default()`). Values are read as the pool reads them
//! (`workers::Workers`, `Placement`, `Smt`, `MemoryPolicy`, `MemoryLock` and
//! `Priority` each parse their text form), so a bad value is refused at start
//! with the setting and what it accepts.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Worker pool settings, from `config.yaml`, `DEFRA_WORKER_*` or `--worker-*`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkersConfig {
    /// `auto`, or how many workers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<String>,
    /// `unpinned`, `spread`, `pack`, or CPU lists split by `;`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    /// `physical` or `all`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smt: Option<String>,
    /// `off`, `local` or `bind:<nodes>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_policy: Option<String>,
    /// `off`, `current`, `future`, each `future` form with `+on-fault`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_lock: Option<String>,
    /// The workers' priority: `inherit`, `rr:<1-99>`, `fifo:<1-99>`,
    /// `nice:<-20..19>` or a macOS QoS class.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// Submissions a worker admits at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbox_items: Option<usize>,
    /// Bytes of submissions a worker admits at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inbox_bytes: Option<usize>,
    /// Microseconds an idle worker spins before it parks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_spin_us: Option<u64>,
    /// Milliseconds a poll may run before it is reported stuck.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stuck_after_ms: Option<u64>,
    /// Seconds the pool has to finish its work at shutdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shutdown_deadline_secs: Option<u64>,
    /// Threads for code that has to block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_threads: Option<usize>,
}

fn parsed<T>(value: &Option<String>) -> Result<Option<T>>
where
    T: std::str::FromStr<Err = workers::Error>,
{
    value
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|error: workers::Error| Error::InvalidConfig(error.to_string()))
}

impl WorkersConfig {
    /// The pool configuration: the pool's defaults with each set value over
    /// them.
    pub fn pool_config(&self) -> Result<workers::Config> {
        let defaults = workers::Config::default();
        let mut priorities = defaults.priorities.clone();
        if let Some(priority) = parsed(&self.priority)? {
            priorities.workers = priority;
        }
        Ok(workers::Config {
            workers: parsed(&self.count)?.unwrap_or(defaults.workers.clone()),
            placement: parsed(&self.placement)?.unwrap_or(defaults.placement.clone()),
            smt: parsed(&self.smt)?.unwrap_or(defaults.smt),
            memory_policy: parsed(&self.memory_policy)?.unwrap_or(defaults.memory_policy.clone()),
            memory_lock: parsed(&self.memory_lock)?.unwrap_or(defaults.memory_lock.clone()),
            priorities,
            inbox_items: self.inbox_items.unwrap_or(defaults.inbox_items),
            inbox_bytes: self.inbox_bytes.unwrap_or(defaults.inbox_bytes),
            idle_spin: self
                .idle_spin_us
                .map_or(defaults.idle_spin, Duration::from_micros),
            stuck_after: self
                .stuck_after_ms
                .map_or(defaults.stuck_after, Duration::from_millis),
            shutdown_deadline: self
                .shutdown_deadline_secs
                .map_or(defaults.shutdown_deadline, Duration::from_secs),
            blocking_threads: self.blocking_threads.unwrap_or(defaults.blocking_threads),
            ..defaults
        })
    }

    /// Refuses a value the pool cannot read, before anything starts.
    pub fn validate(&self) -> Result<()> {
        self.pool_config().map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_settings_keep_the_pool_defaults_and_set_ones_apply() {
        let defaults = workers::Config::default();
        let unset = WorkersConfig::default().pool_config().unwrap();
        assert_eq!(unset.workers, defaults.workers);
        assert_eq!(unset.placement, defaults.placement);
        assert_eq!(unset.priorities, defaults.priorities);
        assert_eq!(unset.inbox_items, defaults.inbox_items);

        let set = WorkersConfig {
            count: Some("1".into()),
            placement: Some("unpinned".into()),
            priority: Some("inherit".into()),
            stuck_after_ms: Some(25),
            ..WorkersConfig::default()
        }
        .pool_config()
        .unwrap();
        assert_eq!(set.workers, workers::Workers::Count(1));
        assert_eq!(set.placement, workers::Placement::Unpinned);
        assert_eq!(set.priorities.workers, workers::Priority::Inherit);
        assert_eq!(set.stuck_after, Duration::from_millis(25));
    }

    #[test]
    fn a_value_the_pool_cannot_read_is_refused_with_the_setting() {
        let error = WorkersConfig {
            placement: Some("everywhere".into()),
            ..WorkersConfig::default()
        }
        .validate()
        .unwrap_err();
        assert!(error.to_string().contains("placement"), "{error}");
    }

    #[test]
    fn an_empty_section_writes_nothing() {
        assert_eq!(
            serde_yaml::to_string(&WorkersConfig::default()).unwrap(),
            "{}\n"
        );
    }
}
