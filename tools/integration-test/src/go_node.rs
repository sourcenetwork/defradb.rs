//! A Go `defradb` node run as a child process on a port it picks itself.
//!
//! The node binds `127.0.0.1:0` and logs the address it got, so no port is
//! chosen for it and nothing can take that port before it binds.

use std::fs::File;
use std::path::{Path, PathBuf};
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

/// How a Go node starts: with P2P listening on `p2p`, and its data in `dir`
/// (else in memory).
#[derive(Clone, Debug, Default)]
pub struct GoOptions {
    pub p2p: Option<String>,
    pub dir: Option<PathBuf>,
}

/// A running Go node, killed when dropped.
pub struct GoNode {
    _process: Process,
    api_url: String,
    _root: TempDir,
}

/// The node's process, killed when dropped, so a failed start leaves nothing
/// running.
struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl GoNode {
    /// Start `binary` as `options` say and wait for its HTTP API.
    pub fn start(binary: &Path, options: &GoOptions) -> Result<Self, GoNodeError> {
        let root = tempfile::tempdir()?;
        let log_path = root.path().join("node.log");
        let log = File::create(&log_path)?;
        let data = match &options.dir {
            Some(dir) => dir.clone(),
            None => root.path().join("data"),
        };
        let store = match options.dir {
            Some(_) => "badger",
            None => "memory",
        };
        let p2p = match &options.p2p {
            Some(address) => vec!["--p2paddr".to_owned(), address.clone()],
            None => vec!["--no-p2p".to_owned()],
        };
        let mut command = Command::new(binary);
        command
            .arg("--rootdir")
            .arg(data)
            .args(["--no-log-color", "start", "--url", "127.0.0.1:0"])
            .args(["--no-keyring", "--store", store, "--no-telemetry"])
            .args(p2p)
            // Encryption and signing are off, as backbone's TestCluster
            // started Go nodes, unless the signed-docs multiplier turns
            // signing on.
            .args(["--no-encryption", "--no-searchable-encryption"]);
        if !signed_docs(std::env::var("DEFRA_MULTIPLIERS").ok().as_deref()) {
            command.arg("--no-signing");
        }
        let mut process = Process(
            command
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?,
        );

        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let text = String::from_utf8_lossy(&std::fs::read(&log_path)?).into_owned();
            if let Some(api_url) = ready_url(&text) {
                return Ok(Self {
                    _process: process,
                    api_url,
                    _root: root,
                });
            }
            if let Some(status) = process.0.try_wait()? {
                return Err(GoNodeError::Exited { status, log: text });
            }
            if Instant::now() >= deadline {
                return Err(GoNodeError::NotReady { log: text });
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn api_url(&self) -> &str {
        &self.api_url
    }
}

/// Whether `DEFRA_MULTIPLIERS` names the `signed-docs` multiplier, which
/// turns signing on, as backbone's `TestCluster` did.
fn signed_docs(multipliers: Option<&str>) -> bool {
    multipliers.is_some_and(|list| list.split(',').any(|m| m.trim() == "signed-docs"))
}

/// The API URL from the node's ready line, once the whole line is written.
fn ready_url(log: &str) -> Option<String> {
    let rest = &log[log.find(READY)? + READY.len()..];
    let line = &rest[..rest.find('\n')?];
    Some(line.split_whitespace().next()?.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{ready_url, signed_docs};

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

    #[test]
    fn signs_only_under_the_signed_docs_multiplier() {
        assert!(signed_docs(Some("signed-docs")));
        assert!(signed_docs(Some("foo, signed-docs")));
        assert!(!signed_docs(Some("foo")));
        assert!(!signed_docs(None));
    }
}
