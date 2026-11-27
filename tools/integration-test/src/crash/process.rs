//! The test's side of a node in its own process.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use regolith::sync::{unbounded, UnboundedReceiver};

use super::host::{DIR_VAR, PREFIX};
use super::{CrashPoint, HOST_TEST};

/// How long the node takes to start, answer a command, or exit.
const DEADLINE: Duration = Duration::from_secs(60);

/// What the node's process says, read off its stdout by a thread that owns
/// the child and reaps it.
enum Said {
    Line(String),
    Exited(ExitStatus),
}

/// A Rust node running in a process of its own, killed when dropped.
pub struct NodeProcess {
    pid: u32,
    url: String,
    stdin: ChildStdin,
    said: UnboundedReceiver<Said>,
    exited: Arc<AtomicBool>,
}

impl NodeProcess {
    /// Start a node on `dir` and wait for its HTTP API.
    pub fn start(dir: &Path) -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| format!("this test binary: {e}"))?;
        let mut child = Command::new(exe)
            .args([HOST_TEST, "--exact", "--ignored", "--nocapture"])
            .args(["--test-threads", "1"])
            .env(DIR_VAR, dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("start the node's process: {e}"))?;
        let pid = child.id();
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tell, said) = unbounded();
        let exited = Arc::new(AtomicBool::new(false));
        let reaped = Arc::clone(&exited);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                // The test harness may print the test's name ahead of it.
                if let Some((_, said)) = line.split_once(PREFIX) {
                    let _ = tell.try_send(Said::Line(said.to_owned()));
                }
            }
            let status = child.wait().expect("reap the node's process");
            reaped.store(true, Ordering::Release);
            let _ = tell.try_send(Said::Exited(status));
        });
        let mut process = Self {
            pid,
            url: String::new(),
            stdin,
            said,
            exited,
        };
        let line = process.line()?;
        process.url = line
            .strip_prefix("url ")
            .ok_or_else(|| format!("the node's process said {line:?}, not its URL"))?
            .to_owned();
        Ok(process)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Arm `point`: the node's next commit kills its process there.
    pub fn arm(&self, point: CrashPoint) -> Result<(), String> {
        self.send(&format!("arm {}", point.name()))?;
        match self.line()?.as_str() {
            "armed" => Ok(()),
            other => Err(format!("the node answered {other:?} to arm")),
        }
    }

    /// Stop the node in order and wait for its process to exit.
    pub fn stop(&self) -> Result<(), String> {
        self.send("stop")?;
        let status = self.exit()?;
        match status.success() {
            true => Ok(()),
            false => Err(format!("the node's process stopped with {status}")),
        }
    }

    /// Kill the node's process with SIGKILL and wait for it to exit.
    pub fn kill(&self) -> Result<(), String> {
        self.signal_kill();
        self.killed()
    }

    /// Wait for the node's process to exit, and check SIGKILL ended it.
    pub fn killed(&self) -> Result<(), String> {
        let status = self.exit()?;
        match killed_by_sigkill(&status) {
            true => Ok(()),
            false => Err(format!(
                "the node's process ended with {status}, not SIGKILL"
            )),
        }
    }

    fn send(&self, command: &str) -> Result<(), String> {
        (&self.stdin)
            .write_all(format!("{command}\n").as_bytes())
            .map_err(|e| format!("send {command:?} to the node's process: {e}"))
    }

    fn next(&self) -> Result<Said, String> {
        workers::block_on(workers::timeout(DEADLINE, self.said.recv_async()))
            .map_err(|e| e.to_string())?
            .map_err(|_| format!("the node's process said nothing for {DEADLINE:?}"))?
            .ok_or_else(|| "the node's process is gone".to_owned())
    }

    fn line(&self) -> Result<String, String> {
        match self.next()? {
            Said::Line(line) => Ok(line),
            Said::Exited(status) => Err(format!("the node's process exited with {status}")),
        }
    }

    fn exit(&self) -> Result<ExitStatus, String> {
        loop {
            if let Said::Exited(status) = self.next()? {
                return Ok(status);
            }
        }
    }

    fn signal_kill(&self) {
        if self.exited.load(Ordering::Acquire) {
            return;
        }
        #[cfg(unix)]
        // SAFETY: kill(2) takes no pointers; the child is not reaped yet, so
        // the ID still names it.
        unsafe {
            libc::kill(self.pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

impl Drop for NodeProcess {
    fn drop(&mut self) {
        self.signal_kill();
    }
}

#[cfg(unix)]
fn killed_by_sigkill(status: &ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal() == Some(libc::SIGKILL)
}

#[cfg(not(unix))]
fn killed_by_sigkill(status: &ExitStatus) -> bool {
    !status.success()
}
