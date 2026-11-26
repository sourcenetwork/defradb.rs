use std::path::Path;

use defra_version::VersionInfo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use crate::client_type::{go_binary, network_edge, ClientType};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// The `node.version` op: what `defradb version` prints, as
/// `{runtime, text, full, json}`: `runtime` is `rust` or `go`, `text` the
/// plain line, `full` the `--full` lines and `json` the `--format json`
/// object.
///
/// In-process it asks the library the CLI prints from. The CLI client runs
/// the Rust binary, and the Rust HTTP client takes `json` from the node's
/// `GET /api/v1/version` and the text from the binary, since the API has no text.
/// Go's HTTP API has no version route, so go-http runs the Go binary.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVersion {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl NodeVersion {
    fn answer(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        match state.client_type() {
            ClientType::InProcess => {
                let info = VersionInfo::new();
                let json =
                    serde_json::to_value(&info).map_err(|e| Failure::operation(e.to_string()))?;
                Ok(answer("rust", info.short(), info.full(), json))
            }
            ClientType::Cli => {
                let binary = crate::rust_binary();
                let json = parse(&rust_version(&binary, &["--format", "json"])?)?;
                Ok(answer(
                    "rust",
                    rust_version(&binary, &[])?,
                    rust_version(&binary, &["--full"])?,
                    json,
                ))
            }
            ClientType::RustHttp => {
                let binary = crate::rust_binary();
                let url = format!("{}/api/v1/version", state.url_on(self.common.node())?);
                Ok(answer(
                    "rust",
                    rust_version(&binary, &[])?,
                    rust_version(&binary, &["--full"])?,
                    fetch(state, url)?,
                ))
            }
            // Go prints its text through cobra's `cmd.Println`, which writes
            // to stderr, and its JSON to stdout.
            ClientType::GoHttp => {
                let binary = go_binary().map_err(|e| Failure::operation(e.to_string()))?;
                Ok(answer(
                    "go",
                    run_version(&binary, &[])?.stderr,
                    run_version(&binary, &["--full"])?.stderr,
                    parse(&run_version(&binary, &["--format", "json"])?.stdout)?,
                ))
            }
        }
    }
}

fn answer(runtime: &str, text: String, full: String, json: Value) -> Value {
    json!({ "runtime": runtime, "text": text, "full": full, "json": json })
}

fn parse(text: &str) -> Result<Value, Failure> {
    serde_json::from_str(text)
        .map_err(|e| Failure::operation(format!("`version` printed no JSON ({e}): {text}")))
}

/// What one `defradb version <args>` run printed to each stream, trimmed.
struct Printed {
    stdout: String,
    stderr: String,
}

fn run_version(binary: &Path, args: &[&str]) -> Result<Printed, Failure> {
    let output = crate::cli_command(binary)
        .arg("version")
        .args(args)
        .output()
        .map_err(|e| Failure::operation(format!("run {}: {e}", binary.display())))?;
    let printed = Printed {
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    };
    match output.status.success() {
        true => Ok(printed),
        false => Err(Failure::operation(format!(
            "`version {}` exited {}: {}",
            args.join(" "),
            output.status,
            printed.stderr
        ))),
    }
}

/// The Rust binary prints its version to stdout and nothing to stderr.
fn rust_version(binary: &Path, args: &[&str]) -> Result<String, Failure> {
    let printed = run_version(binary, args)?;
    match printed.stderr.is_empty() {
        true => Ok(printed.stdout),
        false => Err(Failure::operation(format!(
            "`version {}` wrote to stderr: {}",
            args.join(" "),
            printed.stderr
        ))),
    }
}

fn fetch(state: &State, url: String) -> Result<Value, Failure> {
    let edge = network_edge();
    let body = state
        .block_on(edge.clone().spawn(async move {
            let response = net::http::Client::new(edge)
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            response.bytes().await.map_err(|e| e.to_string())
        }))
        .expect("run on the network edge")
        .map_err(Failure::operation)?;
    parse(&String::from_utf8_lossy(&body))
}

impl Op for NodeVersion {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.answer(state)
    }
}

impl Action for NodeVersion {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
