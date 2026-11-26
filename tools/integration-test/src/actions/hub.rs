//! `defradb hub` run as the process an operator starts (`defra.cli.hub`):
//! started on a rootdir of its own, stopped with a signal, and started again
//! on the same rootdir. The hub is the Rust CLI binary under every client.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use regolith::sync::SegQueue;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tempfile::TempDir;
use testo::prelude::*;

use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// How long a stopped hub may take to exit.
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

/// A hub's rootdir, kept across a stop so a start reuses its key file, and
/// its process while it runs.
pub struct CliHub {
    root: TempDir,
    running: SegQueue<Child>,
}

impl Drop for CliHub {
    fn drop(&mut self) {
        while let Some(mut child) = self.running.pop() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The signal a hub is stopped with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HubSignal {
    /// SIGINT, as Ctrl-C sends.
    #[default]
    Interrupt,
    /// SIGTERM, as a service manager sends.
    Terminate,
}

/// Starts `defradb hub --bind 127.0.0.1:0` on the rootdir of hub `name`,
/// made on its first start, and answers `{"address", "peerId"}` from the
/// address it prints. The key file is the default `<rootdir>/hub.key`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartCliHub {
    pub name: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for StartCliHub {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let binary = crate::rust_binary();
        if !binary.is_file() {
            return Err(Failure::unsupported(format!(
                "{}: {} is not built (`cargo build -p cli`, or set DEFRA_RUST_BINARY)",
                Self::name(),
                binary.display()
            )));
        }
        let hub = match state.cli_hubs.get(&self.name) {
            Some(hub) => Arc::clone(hub),
            None => {
                let root = tempfile::tempdir().map_err(|e| {
                    Failure::operation(format!("could not make the hub's rootdir: {e}"))
                })?;
                let hub = Arc::new(CliHub {
                    root,
                    running: SegQueue::new(),
                });
                state.cli_hubs.insert(self.name.clone(), Arc::clone(&hub));
                hub
            }
        };
        let mut child = Command::new(&binary)
            .args(["hub", "--bind", "127.0.0.1:0", "--no-telemetry"])
            // Logs go to stderr, so standard output carries the address alone.
            .args(["--log-output", "stderr", "--rootdir"])
            .arg(hub.root.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Failure::operation(format!("could not start the hub: {e}")))?;
        let stdout = child.stdout.take();
        hub.running.push(child);
        let mut line = String::new();
        if let Some(stdout) = stdout {
            BufReader::new(stdout)
                .read_line(&mut line)
                .map_err(|e| Failure::operation(format!("could not read the hub address: {e}")))?;
        }
        let address = line.trim();
        let Some((peer_id, _)) = address.split_once('@') else {
            return Err(Failure::operation(format!(
                "the hub printed no address: {address:?}"
            )));
        };
        Ok(json!({ "address": address, "peerId": peer_id }))
    }
}

impl Action for StartCliHub {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// Sends hub `name` a signal and waits for it to exit, answering
/// `{"exitCode"}`. Its rootdir stays for a later `StartCliHub`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopCliHub {
    pub name: String,
    #[serde(default)]
    pub signal: HubSignal,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for StopCliHub {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let mut child = state
            .cli_hubs
            .get(&self.name)
            .and_then(|hub| hub.running.pop())
            .ok_or_else(|| Failure::operation(format!("no running hub {}", self.name)))?;
        signal(&child, self.signal)?;
        let deadline = Instant::now() + EXIT_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Failure::operation(format!(
                        "the hub did not exit within {EXIT_TIMEOUT:?} of {:?}",
                        self.signal
                    )));
                }
                Err(e) => return Err(Failure::operation(format!("waiting on the hub: {e}"))),
            }
        };
        Ok(json!({ "exitCode": status.code() }))
    }
}

impl Action for StopCliHub {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// Sends the signal with kill(1): the child is not reaped yet, so its pid
/// still names it.
#[cfg(unix)]
fn signal(child: &Child, signal: HubSignal) -> Result<(), Failure> {
    let name = match signal {
        HubSignal::Interrupt => "INT",
        HubSignal::Terminate => "TERM",
    };
    let status = Command::new("kill")
        .args(["-s", name, &child.id().to_string()])
        .status()
        .map_err(|e| Failure::operation(format!("could not run kill: {e}")))?;
    match status.success() {
        true => Ok(()),
        false => Err(Failure::operation(format!(
            "kill -s {name} {} failed: {status}",
            child.id()
        ))),
    }
}

#[cfg(not(unix))]
fn signal(_: &Child, signal: HubSignal) -> Result<(), Failure> {
    Err(Failure::unsupported(format!(
        "StopCliHub {signal:?}: signals are Unix only"
    )))
}
