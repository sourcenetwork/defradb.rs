use std::fmt::Debug;
use std::process::{Command, Output};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use testo::prelude::*;

use crate::test_case::State;

/// An action's body for the CLI client.
pub trait CliRun {
    fn execute_cli(&self, state: &mut State);
}

/// `A` run through `defra client` commands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cli<A>(pub A);

impl<A> Action for Cli<A>
where
    A: CliRun + Clone + Debug + Serialize + 'static,
{
    type TState = State;

    fn execute(&self, state: &mut State) {
        self.0.execute_cli(state);
    }
}

impl State {
    /// `State::defra_client` against node `name`, in transaction number
    /// `txn` when the action names one.
    pub fn defra_client_on(
        &self,
        name: &str,
        txn: Option<usize>,
        args: &[&str],
    ) -> Result<Value, String> {
        self.defra_client_as(name, txn, None, args)
    }

    /// [`State::defra_client_on`] as client identity `identity` when the
    /// action names one.
    pub fn defra_client_as(
        &self,
        name: &str,
        txn: Option<usize>,
        identity: Option<u64>,
        args: &[&str],
    ) -> Result<Value, String> {
        self.defra_client_output(name, txn, identity, args)
            .and_then(|output| client_result(&output))
    }

    /// [`State::defra_client_as`], keeping the exit status of a failed run.
    pub fn defra_client_status_as(
        &self,
        name: &str,
        txn: Option<usize>,
        identity: Option<u64>,
        args: &[&str],
    ) -> Result<Value, CliFailure> {
        let output = self
            .defra_client_output(name, txn, identity, args)
            .map_err(|stderr| CliFailure {
                stderr,
                status: None,
            })?;
        client_result(&output).map_err(|stderr| CliFailure {
            stderr,
            status: output.status.code(),
        })
    }

    /// `defra client query` as [`State::defra_client_as`] runs it: what
    /// `query_result` makes of its output.
    pub fn defra_query_as(
        &self,
        name: &str,
        txn: Option<usize>,
        identity: Option<u64>,
        args: &[&str],
    ) -> Result<Value, String> {
        self.defra_client_output(name, txn, identity, args)
            .and_then(|output| query_result(&output))
    }

    fn defra_client_output(
        &self,
        name: &str,
        txn: Option<usize>,
        identity: Option<u64>,
        args: &[&str],
    ) -> Result<Output, String> {
        let url = self.url_on(name).map_err(|failure| failure.message)?;
        let txn = self.txn_id(txn);
        let key = identity.map(crate::scenario::identity::private_key_hex);
        let mut all = Vec::with_capacity(args.len() + 4);
        if let Some(id) = &txn {
            all.extend(["--tx", id.as_str()]);
        }
        if let Some(key) = &key {
            all.extend(["--identity", key.as_str()]);
        }
        all.extend_from_slice(args);
        Ok(self.defra_client(&url, &all))
    }

    /// Runs `defra client <args>` against the node at `url`. The root
    /// directory holds no config, so only the defaults and `--url` apply.
    fn defra_client(&self, url: &str, args: &[&str]) -> Output {
        let binary = crate::rust_binary();
        let rootdir = std::env::temp_dir().join("defra-cli-client-no-config");
        let address = url.trim_start_matches("http://");
        Command::new(&binary)
            .arg("--rootdir")
            .arg(&rootdir)
            .args(["--url", address, "client"])
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("run {}: {e}", binary.display()))
    }
}

/// The GraphQL response `defra client query` printed, which it prints with
/// its errors too, or what it printed to stderr when it printed no response.
/// A response with errors must come with a non-zero exit and `Error: server
/// returned error: <the messages, joined by "; ">` on stderr, and one without
/// errors with a zero exit (`defra.cli.query.error_code`).
fn query_result(output: &Output) -> Result<Value, String> {
    let response = match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(response) if response.is_object() => response,
        _ => return client_result(output),
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let messages: Vec<&str> = response["errors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|error| error["message"].as_str())
        .collect();
    let reported = format!("Error: server returned error: {}", messages.join("; "));
    match (messages.is_empty(), output.status.success()) {
        (true, true) => Ok(response),
        (true, false) => Err(stderr.trim().to_owned()),
        (false, true) => Err(format!(
            "`defra client query` printed errors but exited zero: {response}"
        )),
        (false, false) if stderr.contains(&reported) => Ok(response),
        (false, false) => Err(format!(
            "`defra client query` printed errors but its stderr lacks {reported:?}: {}",
            stderr.trim()
        )),
    }
}

/// What a `defra client` command printed as JSON (null when it printed
/// nothing, its text when that is not JSON), or what it printed to stderr
/// when it failed.
fn client_result(output: &Output) -> Result<Value, String> {
    match output.status.success() {
        true if output.stdout.trim_ascii().is_empty() => Ok(Value::Null),
        true => Ok(serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            Value::String(String::from_utf8_lossy(&output.stdout).trim().to_owned())
        })),
        false => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    use serde_json::json;

    use super::query_result;

    const REFUSED: &str = r#"{"errors": [{"message": "a"}, {"message": "b"}], "data": null}"#;

    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn errors_with_a_failed_exit_and_their_stderr_are_the_response() {
        let printed = output(1, REFUSED, "Error: server returned error: a; b\n");
        assert_eq!(
            query_result(&printed).unwrap()["errors"][1],
            json!({"message": "b"})
        );
    }

    #[test]
    fn errors_with_a_zero_exit_fail() {
        let printed = output(0, REFUSED, "");
        assert!(query_result(&printed).unwrap_err().contains("exited zero"));
    }

    #[test]
    fn errors_without_their_stderr_fail() {
        let printed = output(1, REFUSED, "Error: something else\n");
        assert!(query_result(&printed).unwrap_err().contains("stderr lacks"));
    }

    #[test]
    fn data_with_a_failed_exit_fails_with_its_stderr() {
        let printed = output(1, r#"{"data": {"User": []}}"#, "Error: boom\n");
        assert_eq!(query_result(&printed).unwrap_err(), "Error: boom");
    }

    #[test]
    fn data_with_a_zero_exit_is_the_response() {
        let printed = output(0, r#"{"data": {"User": []}}"#, "");
        assert_eq!(
            query_result(&printed).unwrap(),
            json!({"data": {"User": []}})
        );
    }
}

/// A `defra client` run that failed: what it printed to stderr and its exit
/// status (`None` when it was killed by a signal or never ran).
#[derive(Clone, Debug)]
pub struct CliFailure {
    pub stderr: String,
    pub status: Option<i32>,
}
