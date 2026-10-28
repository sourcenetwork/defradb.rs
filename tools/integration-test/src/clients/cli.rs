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
    /// Runs `defra client <args>` against the node: what it printed as JSON,
    /// or what it printed to stderr when it failed. The root directory holds
    /// no config, so only the defaults and `--url` apply.
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
            true => serde_json::from_slice(&output.stdout)
                .map_err(|e| format!("defra client printed no JSON: {e}")),
            false => Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()),
        }
    }
}
