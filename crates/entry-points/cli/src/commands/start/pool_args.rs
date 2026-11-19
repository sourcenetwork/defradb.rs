//! `defradb start`'s worker pool flags, and the command they ride on.
//!
//! [`StartCommand`] is what `defradb start` parses: every [`StartArgs`] flag
//! plus the `--worker-*` flags, which fill the config's `workers` section
//! (`crate::config::WorkersConfig`). It reads as its [`StartArgs`] through
//! `Deref`, so code holding the parsed command reaches every start flag
//! directly.

use std::ops::Deref;

use clap::Args;

use super::StartArgs;
use crate::config::Config;
use crate::error::Result;

/// The worker pool flags; each unset one keeps the config file's value, or
/// the pool's default.
#[derive(Args, Debug, Default, Clone)]
pub struct WorkerArgs {
    /// Worker threads for the data path: "auto" or a count
    #[arg(long = "workers", env = "DEFRA_WORKERS")]
    pub workers: Option<String>,

    /// Where workers run: unpinned, spread, pack, or CPU lists split by ';'
    #[arg(long, env = "DEFRA_WORKER_PLACEMENT")]
    pub worker_placement: Option<String>,

    /// Whether workers use SMT siblings: physical or all
    #[arg(long, env = "DEFRA_WORKER_SMT")]
    pub worker_smt: Option<String>,

    /// Each worker's NUMA memory policy: off, local or bind:NODES
    #[arg(long, env = "DEFRA_WORKER_MEMORY_POLICY")]
    pub worker_memory_policy: Option<String>,

    /// Lock the process's memory: off, current, future, each future form
    /// with +on-fault
    #[arg(long, env = "DEFRA_WORKER_MEMORY_LOCK")]
    pub worker_memory_lock: Option<String>,

    /// Worker priority: inherit, rr:<1-99>, fifo:<1-99>, nice:<-20..19> or a
    /// macOS QoS class
    #[arg(long, env = "DEFRA_WORKER_PRIORITY")]
    pub worker_priority: Option<String>,

    /// Submissions a worker admits at once
    #[arg(long, env = "DEFRA_WORKER_INBOX_ITEMS")]
    pub worker_inbox_items: Option<usize>,

    /// Bytes of submissions a worker admits at once
    #[arg(long, env = "DEFRA_WORKER_INBOX_BYTES")]
    pub worker_inbox_bytes: Option<usize>,

    /// Microseconds an idle worker spins before it parks
    #[arg(long, env = "DEFRA_WORKER_IDLE_SPIN_US")]
    pub worker_idle_spin_us: Option<u64>,

    /// Milliseconds a poll may run before it is reported stuck
    #[arg(long, env = "DEFRA_WORKER_STUCK_AFTER_MS")]
    pub worker_stuck_after_ms: Option<u64>,

    /// Seconds the pool has to finish its work at shutdown
    #[arg(long, env = "DEFRA_WORKER_SHUTDOWN_DEADLINE_SECS")]
    pub worker_shutdown_deadline_secs: Option<u64>,

    /// Threads for code that has to block
    #[arg(long, env = "DEFRA_WORKER_BLOCKING_THREADS")]
    pub worker_blocking_threads: Option<usize>,
}

impl WorkerArgs {
    /// Sets each given flag in `config.workers` and refuses a value the pool
    /// cannot read.
    pub fn apply_to_config(&self, config: &mut Config) -> Result<()> {
        let section = &mut config.workers;
        let strings = [
            (&self.workers, &mut section.count),
            (&self.worker_placement, &mut section.placement),
            (&self.worker_smt, &mut section.smt),
            (&self.worker_memory_policy, &mut section.memory_policy),
            (&self.worker_memory_lock, &mut section.memory_lock),
            (&self.worker_priority, &mut section.priority),
        ];
        for (flag, setting) in strings {
            if let Some(value) = flag {
                *setting = Some(value.clone());
            }
        }
        if self.worker_inbox_items.is_some() {
            section.inbox_items = self.worker_inbox_items;
        }
        if self.worker_inbox_bytes.is_some() {
            section.inbox_bytes = self.worker_inbox_bytes;
        }
        if self.worker_idle_spin_us.is_some() {
            section.idle_spin_us = self.worker_idle_spin_us;
        }
        if self.worker_stuck_after_ms.is_some() {
            section.stuck_after_ms = self.worker_stuck_after_ms;
        }
        if self.worker_shutdown_deadline_secs.is_some() {
            section.shutdown_deadline_secs = self.worker_shutdown_deadline_secs;
        }
        if self.worker_blocking_threads.is_some() {
            section.blocking_threads = self.worker_blocking_threads;
        }
        section.validate()
    }
}

/// `defradb start`: its flags and the worker pool's.
#[derive(Args, Debug)]
pub struct StartCommand {
    #[command(flatten)]
    pub args: StartArgs,
    #[command(flatten)]
    pub workers: WorkerArgs,
}

impl Deref for StartCommand {
    type Target = StartArgs;

    fn deref(&self) -> &StartArgs {
        &self.args
    }
}

impl StartCommand {
    /// Applies every start flag and every worker flag to `config`.
    pub fn apply_to_config(&self, config: &mut Config) -> Result<()> {
        self.args.apply_to_config(config)?;
        self.workers.apply_to_config(config)
    }

    /// Runs `defradb start`, serving on the network runtime `edge`.
    pub async fn execute(self, mut config: Config, edge: edge::Edge) -> Result<()> {
        self.workers.apply_to_config(&mut config)?;
        self.args.execute(config, edge).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        command: StartCommand,
    }

    #[test]
    fn worker_flags_fill_the_workers_section() {
        let parsed = Wrapper::try_parse_from([
            "defra",
            "--workers=2",
            "--worker-placement=unpinned",
            "--worker-priority=nice:-5",
            "--worker-stuck-after-ms=20",
            "--max-txn-retries=3",
        ])
        .unwrap();
        let mut config = Config::default();
        parsed.command.apply_to_config(&mut config).unwrap();
        assert_eq!(config.workers.count.as_deref(), Some("2"));
        assert_eq!(config.datastore.max_txn_retries, 3);
        let pool = config.workers.pool_config().unwrap();
        assert_eq!(pool.workers, workers::Workers::Count(2));
        assert_eq!(pool.placement, workers::Placement::Unpinned);
        assert_eq!(pool.priorities.workers, workers::Priority::Nice(-5));
        assert_eq!(pool.stuck_after, std::time::Duration::from_millis(20));
    }

    #[test]
    fn a_bad_worker_flag_is_refused_before_startup() {
        let parsed = Wrapper::try_parse_from(["defra", "--workers=0"]).unwrap();
        assert!(parsed
            .command
            .apply_to_config(&mut Config::default())
            .is_err());
    }
}
