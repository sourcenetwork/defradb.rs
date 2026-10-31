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
use crate::nodes::{NodeSpec, TestNode};
use crate::scenario::{Bindings, Failure};

/// What every action sees: the test's nodes, reached through its client type.
#[derive(Clone, Default)]
pub struct State {
    pub client_type: Option<ClientType>,
    pub nodes: Nodes,
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

/// A node name's start options and, while it runs, the node.
#[derive(Clone)]
struct Slot {
    spec: NodeSpec,
    node: Option<Arc<TestNode>>,
}

/// The test's nodes by name. A plain owned map, so stopping a node drops
/// the last handle to its store at once and a restart can reopen it.
#[derive(Clone, Default)]
pub struct Nodes(RapidHashMap<String, Slot>);

impl State {
    fn slot(&self, name: &str) -> Option<&Slot> {
        self.nodes.0.get(name)
    }

    fn client_type(&self) -> ClientType {
        self.client_type
            .expect("no client type: the Connect action did not run")
    }

    /// The running node `name`.
    pub fn node(&self, name: &str) -> Result<Arc<TestNode>, Failure> {
        self.slot(name)
            .and_then(|slot| slot.node.clone())
            .ok_or_else(|| Failure::operation(format!("node {name} is not running")))
    }

    /// The running node `name` as the in-process client calls it.
    pub fn node_on(&self, name: &str) -> Result<Arc<Node>, Failure> {
        self.node(name)?.node.clone().ok_or_else(|| {
            Failure::operation(format!(
                "node {name} has no in-process node: this client reaches it by URL"
            ))
        })
    }

    /// The URL the HTTP and CLI clients reach the running node `name` at.
    pub fn url_on(&self, name: &str) -> Result<String, Failure> {
        self.node(name)?.url.clone().ok_or_else(|| {
            Failure::operation(format!(
                "node {name} has no URL: this client calls it in-process"
            ))
        })
    }

    /// Start node `name` as `spec` says, closing any node of that name first.
    pub fn start_node(&mut self, name: &str, spec: NodeSpec) {
        self.stop_node(name, false);
        let node = Arc::new(TestNode::start(self.client_type(), spec.clone()));
        let slot = Slot {
            spec,
            node: Some(node),
        };
        self.nodes.0.insert(name.to_owned(), slot);
    }

    /// Stop node `name`; with `keep` its data and addresses stay for a restart.
    pub fn stop_node(&mut self, name: &str, keep: bool) {
        let Some(slot) = self.nodes.0.remove(name) else {
            return;
        };
        let mut spec = slot.spec;
        if let Some(node) = slot.node {
            if keep && spec.network {
                spec.listen = node
                    .peer_info()
                    .unwrap_or_default()
                    .iter()
                    .map(|address| address.split("/p2p/").next().unwrap_or(address).to_owned())
                    .collect();
            }
            self.block_on(node.stop());
        }
        if keep {
            self.nodes
                .0
                .insert(name.to_owned(), Slot { spec, node: None });
        }
    }

    /// Restart node `name`: start it again when it is stopped; stop and start
    /// it when it keeps its data; leave an in-memory node as it is, as Go's
    /// harness does.
    pub fn restart_node(&mut self, name: &str) -> Result<(), Failure> {
        let (running, persistent) = self
            .slot(name)
            .map(|slot| (slot.node.is_some(), slot.spec.dir.is_some()))
            .ok_or_else(|| Failure::operation(format!("no node {name} to restart")))?;
        if running {
            if !persistent {
                return Ok(());
            }
            self.stop_node(name, true);
        }
        let spec = self
            .slot(name)
            .map(|slot| slot.spec.clone())
            .unwrap_or_default();
        self.start_node(name, spec);
        Ok(())
    }

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
