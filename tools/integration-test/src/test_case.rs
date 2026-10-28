use std::any::Any;
use std::future::Future;
use std::sync::Arc;

use client::Client;
use rapidhash::RapidHashMap;
use serde_json::Value;
use testo::multiplier::get_active_multipliers;
use testo::prelude::*;

use crate::actions::Actions;
use crate::client_type::ClientType;
use crate::export;
use crate::scenario::Bindings;

/// What every action sees: the database client, and whatever must stay alive
/// for it to keep working (a server, a child process).
#[derive(Clone, Default)]
pub struct State {
    pub client: Option<Arc<dyn Client>>,
    pub held: Vec<Arc<dyn Any + Send + Sync>>,
    pub captures: RapidHashMap<String, Value>,
    pub bindings: Bindings,
}

impl State {
    pub fn client(&self) -> &dyn Client {
        self.client
            .as_deref()
            .expect("no client: the Connect action did not run")
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        workers::block_on(future).expect("run on a worker")
    }
}

/// A Go-style integration test: a list of actions run in order against one
/// fresh database, once for each client type selected by `DEFRA_CLIENT`.
#[derive(Default)]
pub struct TestCase {
    /// The scenario's name when exported, from `scenario_id!()`.
    pub id: &'static str,
    pub actions: Vec<ActionInstance<Actions>>,
    /// Why the upstream test is skipped; the test is not run.
    pub skip: Option<&'static str>,
    /// The scenario never opens the default `db` node.
    pub without_db: bool,
}

impl TestCase {
    /// The actions, without the sync/async wrapper.
    pub fn actions(&self) -> Vec<Actions> {
        self.actions
            .iter()
            .map(|instance| match instance {
                ActionInstance::Sync(action) | ActionInstance::Async(action) => action.clone(),
            })
            .collect()
    }

    pub fn from_actions(actions: Vec<Actions>) -> Self {
        Self {
            actions: actions.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    pub fn execute(self) {
        if let Some(dir) = std::env::var_os("DEFRA_EXPORT_SCENARIOS") {
            self.export(std::path::Path::new(&dir));
            return;
        }
        self.check_export();
        if let Some(reason) = self.skip {
            eprintln!("skipped: {reason}");
            return;
        }
        for client_type in active_client_types() {
            let actions = client_type.apply(&self.actions);
            drop(testo::action::execute(State::default(), &actions));
        }
    }
}

impl TestCase {
    fn export(&self, dir: &std::path::Path) {
        let steps: Vec<_> = self.actions().iter().flat_map(Actions::wires).collect();
        let id = self.id.split("::").skip(1).collect::<Vec<_>>().join(".");
        let scenario = export::scenario(&id, &steps, self.without_db, self.skip);
        export::write(dir, &id, &scenario);
    }

    fn check_export(&self) {
        let steps: Vec<_> = self.actions().iter().flat_map(Actions::wires).collect();
        if self.id.is_empty() || steps.is_empty() {
            return;
        }
        let scenario = export::scenario(self.id, &steps, self.without_db, self.skip);
        if let Err(reason) = export::check(&scenario, self.without_db) {
            panic!("{}: {reason}", self.id);
        }
    }
}

/// The client types named by `DEFRA_CLIENT` (comma separated), or in-process.
fn active_client_types() -> Vec<ClientType> {
    let requested = std::env::var("DEFRA_CLIENT").unwrap_or_default();
    let requested: Vec<&str> = requested
        .split(',')
        .filter(|name| !name.is_empty())
        .collect();
    if requested.is_empty() {
        return vec![ClientType::InProcess];
    }
    let all = ClientType::ALL;
    let possible: Vec<&ClientType> = all.iter().collect();
    let active =
        get_active_multipliers::<State, Actions, Actions, ClientType>("DEFRA_CLIENT", &possible);
    assert_eq!(
        active.len(),
        requested.len(),
        "DEFRA_CLIENT={requested:?}: expected names from in-process, rust-http, go-http"
    );
    active.into_iter().cloned().collect()
}
