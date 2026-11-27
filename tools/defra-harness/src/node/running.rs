use std::path::PathBuf;
use std::time::Duration;

use crate::error::{Error, Result, WrapErr};

use super::{DefraNode, NodeConfig};
use crate::divergences::NodeKind;
use crate::observe::patterns::{self, NamedPattern};
use crate::observe::LogTracker;

/// A running DefraDB node with its process handle and log tracker.
pub struct RunningNode {
    pub name: String,
    pub api_url: String,
    pub http_addr: String,
    pub binary_path: PathBuf,
    pub process: crate::infra::ManagedProcess,
    pub log_tracker: LogTracker,
    pub rootdir: PathBuf,
    pub(crate) config: NodeConfig,
    pub(crate) kind: NodeKind,
}

/// Start a DefraDB node from config and wait for it to become ready.
pub async fn start_node(
    node: &dyn DefraNode,
    config: NodeConfig,
    ready_timeout: Duration,
) -> Result<RunningNode> {
    std::fs::create_dir_all(&config.rootdir)?;
    std::fs::create_dir_all(&config.log_dir)?;

    // Seed a cluster-shared searchable-encryption key into the keyring before
    // start so the node's getOrCreate (Go + Rust) finds the same key.
    super::seed_searchable_encryption_key(node.binary_path(), node.kind(), &config)
        .wrap_err_with(|| format!("{}: failed to seed searchable-encryption key", config.name))?;

    let api_url = format!("http://{}", config.http_addr);
    let named_patterns: Vec<NamedPattern> = if config.p2p_transport.as_deref() == Some("iroh") {
        patterns::iroh_patterns()
    } else {
        patterns::node_patterns()
    };

    let (program, args_owned, envs_owned) = node.command_parts(&config);
    let args: Vec<&str> = args_owned.iter().map(|s| s.as_str()).collect();
    let envs: Vec<(&str, &str)> = envs_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut process =
        crate::infra::ManagedProcess::spawn(&config.name, &program, &args, &envs, &config.log_dir)?;

    let stdout_path = config.log_dir.join("stdout.log");
    let log_tracker = LogTracker::start(stdout_path, patterns::DEFRA_READY_PATTERN, named_patterns);

    wait_ready_or_exit(&log_tracker, &mut process, &config, ready_timeout).await?;

    Ok(RunningNode {
        name: config.name.clone(),
        api_url,
        http_addr: config.http_addr.clone(),
        binary_path: node.binary_path().to_path_buf(),
        process,
        log_tracker,
        rootdir: config.rootdir.clone(),
        config,
        kind: node.kind(),
    })
}

/// Wait for the ready pattern, failing fast if the child exits first.
///
/// A stolen port kills the child within milliseconds of spawn; without exit
/// detection the caller would burn the full `ready_timeout` before learning
/// anything, and the logs explaining why would never surface in the error.
async fn wait_ready_or_exit(
    log_tracker: &LogTracker,
    process: &mut crate::infra::ManagedProcess,
    config: &NodeConfig,
    ready_timeout: Duration,
) -> Result<()> {
    let ready = log_tracker.wait_for_ready(ready_timeout);
    tokio::pin!(ready);
    loop {
        tokio::select! {
            r = &mut ready => {
                return r.wrap_err_with(|| format!("{}: did not become ready", config.name));
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if process.is_running() {
                    continue;
                }
                // The tail loop polls the log file every 10ms, so the ready
                // line may be written but not yet delivered. Grace-wait
                // before declaring the start failed.
                if let Ok(Ok(())) =
                    tokio::time::timeout(Duration::from_millis(300), &mut ready).await
                {
                    return Ok(());
                }
                let tail = log_tail(config);
                let err = crate::error::err!(
                    "{}: process exited before becoming ready\n{}",
                    config.name,
                    tail
                );
                if tail_indicates_port_conflict(&tail) {
                    return Err(Error::PortConflict(Box::new(err)));
                }
                return Err(err);
            }
        }
    }
}

/// Last lines of the node's stderr/stdout logs, for exit diagnostics.
fn log_tail(config: &NodeConfig) -> String {
    const TAIL_LINES: usize = 15;
    let mut out = String::new();
    for file in ["stderr.log", "stdout.log"] {
        let Ok(content) = std::fs::read_to_string(config.log_dir.join(file)) else {
            continue;
        };
        let mut tail: Vec<&str> = content.lines().rev().take(TAIL_LINES).collect();
        tail.reverse();
        if !tail.is_empty() {
            out.push_str(&format!(
                "--- {} (last {} lines) ---\n{}\n",
                file,
                tail.len(),
                tail.join("\n")
            ));
        }
    }
    if out.is_empty() {
        out.push_str("(no log output captured)");
    }
    out
}

/// Rust bind errors read "Address already in use"; Go's read
/// "bind: address already in use"; tokio/libp2p variants surface
/// `AddrInUse`. All collapse under a lowercase substring check.
fn tail_indicates_port_conflict(tail: &str) -> bool {
    let lower = tail.to_lowercase();
    lower.contains("address already in use") || lower.contains("addrinuse")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct StubNode {
        script: String,
        binary: PathBuf,
    }

    impl StubNode {
        fn new(script: &str) -> Self {
            Self {
                script: script.to_string(),
                binary: PathBuf::from("/bin/sh"),
            }
        }
    }

    impl DefraNode for StubNode {
        fn kind(&self) -> NodeKind {
            NodeKind::Rust
        }

        fn command_parts(
            &self,
            _config: &NodeConfig,
        ) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
            (
                self.binary.clone(),
                vec!["-c".to_string(), self.script.clone()],
                vec![],
            )
        }

        fn binary_path(&self) -> &Path {
            &self.binary
        }
    }

    fn test_config(label: &str) -> NodeConfig {
        let dir = std::env::temp_dir().join(format!(
            "defra-harness-running-test-{}-{}",
            label,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        NodeConfig::new(
            format!("stub-{}", label),
            dir.join("data"),
            dir.join("logs"),
            "127.0.0.1:0",
        )
    }

    #[tokio::test]
    async fn port_conflict_exit_fails_fast_and_is_classified() {
        let node = StubNode::new(
            "echo 'Error: failed to listen: Address already in use (os error 48)' >&2; exit 1",
        );
        let config = test_config("addrinuse");
        let started = std::time::Instant::now();
        let err = match start_node(&node, config, Duration::from_secs(30)).await {
            Ok(_) => panic!("bind-conflict exit must fail"),
            Err(e) => e,
        };
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "must fail fast on child exit, not wait out the ready timeout (took {:?})",
            started.elapsed()
        );
        assert!(
            err.is_port_conflict(),
            "error must be classified as a port conflict: {err:?}"
        );
        assert!(
            format!("{err:?}").contains("Address already in use"),
            "error must include the child's log tail: {err:?}"
        );
    }

    #[tokio::test]
    async fn unrelated_exit_fails_fast_without_port_conflict_marker() {
        let node = StubNode::new("echo 'panic: something unrelated' >&2; exit 2");
        let config = test_config("unrelated");
        let started = std::time::Instant::now();
        let err = match start_node(&node, config, Duration::from_secs(30)).await {
            Ok(_) => panic!("non-zero exit must fail"),
            Err(e) => e,
        };
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(
            !err.is_port_conflict(),
            "unrelated failures must not be classified as port conflicts: {err:?}"
        );
    }

    #[tokio::test]
    async fn ready_line_before_exit_still_counts_as_started() {
        let node = StubNode::new("echo 'Providing HTTP API at 127.0.0.1:0'; sleep 30");
        let config = test_config("ready");
        let running = start_node(&node, config, Duration::from_secs(10))
            .await
            .expect("ready node must start");
        drop(running);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restart_waits_for_new_readiness_not_a_stale_log() {
        let node = StubNode::new(
            "echo 'new process starting'; sleep 0.2; echo 'Providing HTTP API at fresh'; sleep 30",
        );
        let config = test_config("stale-ready");
        std::fs::create_dir_all(&config.log_dir).unwrap();
        std::fs::write(
            config.log_dir.join("stdout.log"),
            format!(
                "Providing HTTP API at stale\n{}",
                "stale output\n".repeat(20_000)
            ),
        )
        .unwrap();

        let running = start_node(&node, config.clone(), Duration::from_secs(5))
            .await
            .expect("new process must become ready");
        let log = std::fs::read_to_string(config.log_dir.join("stdout.log")).unwrap();
        assert!(log.contains("Providing HTTP API at fresh"));
        drop(running);
    }
}
