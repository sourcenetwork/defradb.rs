//! Starting a `defradb` child on an HTTP port it binds itself.
//!
//! Picking a free port, releasing it and passing it to the child leaves a
//! window in which another process can take it (#2798). The child binds
//! [`ANY_PORT`] instead, so the OS picks the port at bind time, and the test
//! learns the address from the line the server logs once it listens: a
//! [`NodeLog`] copies the child's output to a file and signals that line.

// Each test binary compiles this module and uses only part of it.
#![allow(dead_code)]

use std::fs::File;
use std::io::{BufRead, BufReader, PipeWriter, Write};
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use kovan_channel::{bounded, RecvDeadline};

/// `--url` for a child that binds any free loopback port.
pub const ANY_PORT: &str = "127.0.0.1:0";

/// Log flags for the child, so the ready line is plain text on the pipe
/// whatever `DEFRA_LOG_FORMAT` or `DEFRA_LOG_OUTPUT` the test inherits.
pub const LOG_FLAGS: [&str; 4] = ["--log-format", "text", "--log-output", "stderr"];

/// The line the HTTP server logs with the address it bound (`server.rs`).
const READY: &str = "Providing HTTP API at ";

/// A child's log: a thread copies the child's output from a pipe into a
/// file, line by line, and signals the address in the first ready line.
pub struct NodeLog {
    path: PathBuf,
    ready: bounded::Receiver<String>,
    reader: JoinHandle<()>,
}

impl NodeLog {
    /// Creates the log file at `path` and returns the log with the pipe end
    /// to give the child as its stderr (and stdout, through `try_clone`).
    /// The caller drops its copies of that end once the child is spawned,
    /// so the pipe closes when the child exits.
    pub fn create(path: PathBuf) -> (Self, PipeWriter) {
        let mut file = File::create(&path).expect("create node log");
        let (output, input) = std::io::pipe().expect("node log pipe");
        let (signal, ready) = bounded(1);
        let reader = std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            let mut signal = Some(signal);
            let mut line = Vec::new();
            loop {
                line.clear();
                match output.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) => {
                        let _ = writeln!(file, "[node log reader stopped: {error}]");
                        break;
                    }
                }
                // Written before the signal, so the line is in the file by
                // the time a waiter wakes.
                file.write_all(&line).expect("write node log");
                if let Some(address) = bound_address(&String::from_utf8_lossy(&line)) {
                    if let Some(signal) = signal.take() {
                        signal.send(address);
                    }
                }
            }
        });
        (
            Self {
                path,
                ready,
                reader,
            },
            input,
        )
    }

    /// Waits up to `deadline` for the node to log the address its HTTP
    /// server bound and returns it as `host:port`. Fails with the log as
    /// soon as the node's output closes or the deadline passes.
    pub fn http_address(&self, deadline: Duration) -> String {
        match self.ready.recv_deadline(Instant::now() + deadline) {
            RecvDeadline::Msg(address) => address,
            RecvDeadline::Disconnected => {
                panic!("node exited before serving HTTP:\n{}", self.text())
            }
            RecvDeadline::Timeout => panic!(
                "node did not serve HTTP within {deadline:?}:\n{}",
                self.text()
            ),
        }
    }

    /// The log so far.
    pub fn text(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// Waits for the child's output to close and returns the whole log.
    pub fn closed(self) -> String {
        let Self { path, reader, .. } = self;
        reader.join().expect("node log reader");
        std::fs::read_to_string(path).unwrap_or_default()
    }
}

/// The address in a complete `READY` line, without its scheme. The address
/// ends at whitespace or at a colour escape.
fn bound_address(line: &str) -> Option<String> {
    if !line.ends_with('\n') {
        return None;
    }
    let url = line.split_once(READY)?.1;
    let url = url
        .split(|c: char| c.is_whitespace() || c == '\x1b')
        .next()?;
    let (_, address) = url.split_once("://")?;
    Some(address.to_string())
}
