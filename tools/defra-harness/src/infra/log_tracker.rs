use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rapidhash::fast::RandomState;
use regex::Regex;
use regolith::sync::{Atom, HopscotchMap, Notify};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::JoinHandle;

use crate::error::{err, Result, WrapErr};

/// How long the tail waits before it looks again for a file that is not
/// there yet, or for bytes past the end of the file. A file is polled: no
/// portable signal says another process appended to it.
const TAIL_POLL: Duration = Duration::from_millis(10);

/// A named log pattern for event-driven test verification.
pub struct NamedPattern {
    pub name: &'static str,
    pub regex: Regex,
}

/// What the tail has read so far. The tail task is the only writer; every
/// change is followed by `changed`, which waiters sleep on.
struct Observed {
    ready: AtomicBool,
    /// The first line that matched each named pattern.
    seen: HopscotchMap<String, String, RandomState>,
    errors: AtomicU64,
    last_error: Atom<String>,
    closed: AtomicBool,
    changed: Notify,
}

impl Observed {
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            seen: HopscotchMap::with_hasher(RandomState::default()),
            errors: AtomicU64::new(0),
            last_error: Atom::new(String::new()),
            closed: AtomicBool::new(false),
            changed: Notify::new(),
        }
    }
}

/// Tails a node's stdout.log and records what it matches.
///
/// Generic over any component — the `ready_pattern` and `named_patterns`
/// are supplied by the consuming harness.
pub struct LogTracker {
    observed: Arc<Observed>,
    task: JoinHandle<()>,
}

impl LogTracker {
    /// Create an empty placeholder (no background task).
    pub fn empty() -> Self {
        Self {
            observed: Arc::new(Observed::new()),
            task: tokio::spawn(async {}),
        }
    }

    /// Start tailing `log_path`, matching the ready pattern and any named patterns.
    ///
    /// `ready_pattern` is a substring match (e.g., `"Providing HTTP API at"` for defra).
    pub fn start(
        log_path: PathBuf,
        ready_pattern: &str,
        named_patterns: Vec<NamedPattern>,
    ) -> Self {
        let observed = Arc::new(Observed::new());
        let tail = Arc::clone(&observed);
        let ready_pattern = ready_pattern.to_string();

        let task = tokio::spawn(async move {
            if let Err(e) = tail_loop(log_path, &ready_pattern, named_patterns, &tail).await {
                tracing::warn!("log tracker stopped: {}", e);
            }
            tail.closed.store(true, Ordering::Release);
            tail.changed.notify_waiters();
        });

        Self { observed, task }
    }

    /// Wait for the ready line or timeout. An `ERROR` line read while waiting
    /// fails the wait.
    pub async fn wait_for_ready(&self, timeout: Duration) -> Result<()> {
        let observed = &self.observed;
        let errors_before = observed.errors.load(Ordering::Acquire);
        let wait = async {
            loop {
                let changed = observed.changed.notified();
                if observed.ready.load(Ordering::Acquire) {
                    return Ok(());
                }
                if observed.errors.load(Ordering::Acquire) != errors_before {
                    return Err(err!("node error: {}", observed.last_error.load_clone()));
                }
                if observed.closed.load(Ordering::Acquire) {
                    return Err(err!("log tracker closed"));
                }
                changed.await;
            }
        };

        match tokio::time::timeout(timeout, wait).await {
            Ok(inner) => inner,
            Err(_) => Err(err!("timed out waiting for node ready")),
        }
    }

    /// Wait for a named pattern to match, returning the matched line.
    ///
    /// If the pattern was already matched before this call, returns immediately.
    pub async fn wait_for_pattern(&self, name: &str, timeout: Duration) -> Result<String> {
        let observed = &self.observed;
        let wait = async {
            loop {
                let changed = observed.changed.notified();
                if let Some(line) = observed.seen.get(name) {
                    return Ok(line);
                }
                if observed.closed.load(Ordering::Acquire) {
                    return Err(err!("log tracker closed"));
                }
                changed.await;
            }
        };

        match tokio::time::timeout(timeout, wait).await {
            Ok(inner) => inner,
            Err(_) => Err(err!("timed out waiting for pattern '{}'", name)),
        }
    }
}

impl Drop for LogTracker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn tail_loop(
    log_path: PathBuf,
    ready_pattern: &str,
    named_patterns: Vec<NamedPattern>,
    observed: &Observed,
) -> Result<()> {
    while !log_path.exists() {
        tokio::time::sleep(TAIL_POLL).await;
    }

    let file = tokio::fs::File::open(&log_path)
        .await
        .wrap_err_with(|| format!("failed to open {}", log_path.display()))?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .await
            .wrap_err("error reading log")?;
        if read == 0 {
            tokio::time::sleep(TAIL_POLL).await;
            continue;
        }
        let mut changed = false;
        if line.contains(ready_pattern) {
            observed.ready.store(true, Ordering::Release);
            changed = true;
        }
        if line.contains("ERROR") {
            observed.last_error.store(line.trim().to_string());
            observed.errors.fetch_add(1, Ordering::AcqRel);
            changed = true;
        }
        for pattern in &named_patterns {
            if pattern.regex.is_match(&line) {
                observed
                    .seen
                    .insert_if_absent(pattern.name.to_string(), line.trim().to_string());
                changed = true;
            }
        }
        if changed {
            observed.changed.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readiness_is_remembered_before_a_waiter_subscribes() {
        let dir = crate::infra::TestRunDir::new(&std::env::temp_dir(), "DEFRA_LOG_TEST_KEEP")
            .expect("create isolated log directory");
        let path = dir.path().join("stdout.log");
        std::fs::write(&path, "server ready\n").expect("write readiness line");
        let tracker = LogTracker::start(
            path,
            "server ready",
            vec![NamedPattern {
                name: "readiness_seen",
                regex: Regex::new("server ready").unwrap(),
            }],
        );
        tracker
            .wait_for_pattern("readiness_seen", Duration::from_secs(2))
            .await
            .expect("tracker must consume the ready line before the wait");
        tracker
            .wait_for_ready(Duration::from_millis(100))
            .await
            .expect("readiness observed before subscription must remain visible");
    }
}
