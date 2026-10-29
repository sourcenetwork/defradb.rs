use std::any::Any;
use std::future::Future;
use std::sync::Arc;

use defra_core::{RequestContext, TxnId};
use defra_node::Node;
use rapidhash::RapidHashMap;
use regolith::sync::Atom;
use serde_json::Value;
use testo::multiplier::get_active_multipliers;
use testo::prelude::*;

use crate::actions::Actions;
use crate::client_type::ClientType;
use crate::export;
use crate::scenario::Bindings;

/// What every action sees: the node an in-process action acts on, or the URL
/// the other clients reach it at; whatever must stay alive for that (a
/// server, a child process); and what earlier actions captured.
#[derive(Clone, Default)]
pub struct State {
    pub node: Option<Arc<Node>>,
    pub url: Option<String>,
    pub held: Vec<Arc<dyn Any + Send + Sync>>,
    pub captures: RapidHashMap<String, Value>,
    pub bindings: Bindings,
    /// The transactions the test began, in order; actions name one by index.
    pub txns: Txns,
}

/// The transactions a test has begun. Shared by every clone of the state, and
/// appended to by an op that only sees the state by reference.
#[derive(Clone)]
pub struct Txns(Arc<Atom<Vec<TxnId>>>);

impl Default for Txns {
    fn default() -> Self {
        Self(Arc::new(Atom::new(Vec::new())))
    }
}

impl Txns {
    pub fn push(&self, id: TxnId) {
        self.0.rcu(|ids| {
            let mut next = ids.clone();
            next.push(id);
            next
        });
    }

    pub fn get(&self, index: usize) -> Option<TxnId> {
        self.0.load_clone().get(index).copied()
    }
}

impl State {
    /// The context a request runs in: inside transaction number `txn` when
    /// the action names one.
    pub fn context(&self, txn: Option<usize>) -> RequestContext {
        let id = txn.map(|index| {
            self.txns
                .get(index)
                .unwrap_or_else(|| panic!("no transaction {index}: TxnBegin did not run"))
        });
        RequestContext::ambient().with_txn(id)
    }

    /// Transaction number `txn`'s ID as the HTTP API and the CLI write it.
    pub fn txn_id(&self, txn: Option<usize>) -> Option<String> {
        self.context(txn).txn().map(|id| id.to_string())
    }

    pub fn node(&self) -> &Node {
        self.node.as_deref().expect(
            "no in-process node: Connect did not run, or this client reaches the node by URL",
        )
    }

    pub fn url(&self) -> &str {
        self.url
            .as_deref()
            .expect("no URL: Connect did not run, or this client calls the node in-process")
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
        "DEFRA_CLIENT={requested:?}: expected names from {}",
        ClientType::names()
    );
    active.into_iter().cloned().collect()
}
