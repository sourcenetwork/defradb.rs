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
    /// [`State::defra_client`] in transaction number `txn`, when the action
    /// names one.
    pub fn defra_client_in(&self, txn: Option<usize>, args: &[&str]) -> Result<Value, String> {
        match self.txn_id(txn) {
            Some(id) => self.defra_client(&[&["--tx", id.as_str()], args].concat()),
            None => self.defra_client(args),
        }
    }

    /// Runs `defra client <args>` against the node: what it printed as JSON
    /// (null when it printed nothing), or what it printed to stderr when it
    /// failed. The root directory holds no config, so only the defaults and
    /// `--url` apply.
    pub fn defra_client(&self, args: &[&str]) -> Result<Value, String> {
        let binary = crate::rust_binary();
        let rootdir = std::env::temp_dir().join("defra-cli-client-no-config");
        let address = self.url().trim_start_matches("http://");
        let output = Command::new(&binary)
            .arg("--rootdir")
            .arg(&rootdir)
            .args(["--url", address, "client"])
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("run {}: {e}", binary.display()));
        match output.status.success() {
            true if output.stdout.trim_ascii().is_empty() => Ok(Value::Null),
            true => serde_json::from_slice(&output.stdout)
                .map_err(|e| format!("defra client printed no JSON: {e}")),
            false => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        }
    }
}
