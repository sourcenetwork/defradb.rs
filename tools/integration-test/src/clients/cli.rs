use std::fmt::Debug;
use std::process::Command;

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
    /// [`State::defra_client`] against node `name`, in transaction number
    /// `txn` when the action names one.
    pub fn defra_client_on(
        &self,
        name: &str,
        txn: Option<usize>,
        args: &[&str],
    ) -> Result<Value, String> {
        let url = self.url_on(name).map_err(|failure| failure.message)?;
        match self.txn_id(txn) {
            Some(id) => self.defra_client(&url, &[&["--tx", id.as_str()], args].concat()),
            None => self.defra_client(&url, args),
        }
    }

    /// Runs `defra client <args>` against the node at `url`: what it printed as JSON
    /// (null when it printed nothing, its text when that is not JSON), or what
    /// it printed to stderr when it
    /// failed. The root directory holds no config, so only the defaults and
    /// `--url` apply.
    fn defra_client(&self, url: &str, args: &[&str]) -> Result<Value, String> {
        let binary = crate::rust_binary();
        let rootdir = std::env::temp_dir().join("defra-cli-client-no-config");
        let address = url.trim_start_matches("http://");
        let output = Command::new(&binary)
            .arg("--rootdir")
            .arg(&rootdir)
            .args(["--url", address, "client"])
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("run {}: {e}", binary.display()));
        match output.status.success() {
            true if output.stdout.trim_ascii().is_empty() => Ok(Value::Null),
            true => Ok(serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
                Value::String(String::from_utf8_lossy(&output.stdout).trim().to_owned())
            })),
            false => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        }
    }
}
