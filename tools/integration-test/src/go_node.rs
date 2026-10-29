//! A Go `defradb` node run as a child process on a port it picks itself.
//!
//! The node binds `127.0.0.1:0` and logs the address it got, so no port is
//! chosen for it and nothing can take that port before it binds.

use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const READY: &str = "Providing HTTP API at ";
const READY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum GoNodeError {
    #[error("could not prepare the Go node: {0}")]
    Setup(#[from] std::io::Error),
    #[error("Go node exited with {status} before it was ready:\n{log}")]
    Exited {
        status: std::process::ExitStatus,
        log: String,
    },
    #[error("Go node was not ready after {READY_TIMEOUT:?}:\n{log}")]
    NotReady { log: String },
}

/// A running Go node, killed when dropped.
pub struct GoNode {
    child: Child,
    api_url: String,
    _root: TempDir,
}

impl GoNode {
    /// Start `binary` with an in-memory store and wait for its HTTP API.
    pub fn start(binary: &Path) -> Result<Self, GoNodeError> {
        let root = tempfile::tempdir()?;
        let log_path = root.path().join("node.log");
        let log = File::create(&log_path)?;
        let mut child = Command::new(binary)
            .arg("--rootdir")
            .arg(root.path().join("data"))
            .args(["--no-log-color", "start", "--url", "127.0.0.1:0"])
            .args([
                "--no-keyring",
                "--store",
                "memory",
                "--no-p2p",
                "--no-telemetry",
            ])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;

        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let text = std::fs::read_to_string(&log_path)?;
            if let Some(api_url) = ready_url(&text) {
                return Ok(Self {
                    child,
                    api_url,
                    _root: root,
                });
            }
            if let Some(status) = child.try_wait()? {
                return Err(GoNodeError::Exited { status, log: text });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GoNodeError::NotReady { log: text });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }
}

impl Drop for GoNode {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The API URL from the node's ready line, once the whole line is written.
fn ready_url(log: &str) -> Option<String> {
    let rest = &log[log.find(READY)? + READY.len()..];
    let line = &rest[..rest.find('\n')?];
    Some(line.split_whitespace().next()?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::ready_url;

    #[test]
    fn reads_the_bound_address_from_the_ready_line() {
        let log = "INF cli Starting DefraDB\n\
                   INF node Providing HTTP API at http://127.0.0.1:58132 ExplorerEnabled=true\n";
        assert_eq!(ready_url(log).as_deref(), Some("http://127.0.0.1:58132"));
    }

    #[test]
    fn waits_for_the_ready_line_to_finish() {
        assert_eq!(
            ready_url("INF node Providing HTTP API at http://127.0.0.1:58"),
            None
        );
        assert_eq!(ready_url("INF cli Starting DefraDB\n"), None);
    }
}
