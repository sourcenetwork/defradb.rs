//! Merge governance installed on a browser node, and the sweep it runs
//! while no peer runs one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::merge::governance::rule::{BlockstoreModules, RuleBudget, RuleEngine, WasmRules};
use db::merge::governance::{
    install_merge_governance, MarkMergedSink, MergeGovernance, SWEEP_INTERVAL,
};
use db::merge::{DbMergeHandler, DEFAULT_MAX_MERGE_DEPTH};
use db::DB;
use futures::channel::oneshot;
use futures::future::{select, Either};
use storage::RegolithStore;

use super::GovernanceConfig;
use crate::error::{Result, WasmError};

type Blocks = DefraBlockstore<RegolithStore>;

/// What a client installed, held for its lifetime.
pub(crate) struct Governance {
    collections: Vec<String>,
    engine: RuleEngine,
    budget: RuleBudget,
    rules: Arc<WasmRules>,
    modules: BlockstoreModules<Blocks>,
    blockstore: Arc<Blocks>,
    /// Modules held through this client, in the order they were put.
    held: Vec<Cid>,
    /// Whether a peer is running, and with it the peer's own sweep.
    peer_running: Arc<AtomicBool>,
    sweep: Option<Sweep>,
}

impl Governance {
    /// Hold the configured modules, claim the collections, judge local
    /// writes, and start the sweep. Called on a database nothing has
    /// written to or merged into yet in this session.
    // Nothing here is Send on wasm32, and nothing needs to be.
    #[allow(clippy::arc_with_non_send_sync)]
    pub(crate) async fn install(
        db: &Arc<DB<RegolithStore>>,
        config: &GovernanceConfig,
    ) -> Result<Self> {
        let (collections, engine, budget) = config.resolve()?;
        // The flavour of blockstore the peer uses, over the same store, so a
        // merge the sweep makes clears the same unmerged index.
        let blockstore = Arc::new(DefraBlockstore::new(Arc::clone(db.store()), true));
        let modules = BlockstoreModules::new(Arc::clone(&blockstore));
        let rules = Arc::new(
            WasmRules::with_engine(
                Arc::new(BlockstoreModules::new(Arc::clone(&blockstore))),
                engine,
                budget,
            )
            .map_err(WasmError::Governance)?,
        );
        let mut held = Vec::with_capacity(config.rule_modules.len());
        for bytes in &config.rule_modules {
            held.push(hold(&rules, &modules, bytes).await?);
        }
        install_merge_governance(
            db,
            Arc::clone(&blockstore),
            MergeGovernance::new(collections.iter().cloned()).with_validator(rules.clone()),
            DEFAULT_MAX_MERGE_DEPTH,
        );
        let installed = db
            .merge_governance()
            .map(|governance| governance.collections() == collections)
            .unwrap_or(false);
        if !installed {
            return Err(WasmError::Governance(
                "this database already has other merge governance installed".to_string(),
            ));
        }
        let peer_running = Arc::new(AtomicBool::new(false));
        let sweep = Sweep::spawn(db, &blockstore, &peer_running);
        Ok(Self {
            collections,
            engine,
            budget,
            rules,
            modules,
            blockstore,
            held,
            peer_running,
            sweep: Some(sweep),
        })
    }

    /// A peer started or stopped: while one runs, its sweep is the one.
    pub(crate) fn set_peer_running(&self, running: bool) {
        self.peer_running.store(running, Ordering::Release);
    }

    pub(crate) async fn put_module(&mut self, bytes: &[u8]) -> Result<Cid> {
        let cid = hold(&self.rules, &self.modules, bytes).await?;
        if !self.held.contains(&cid) {
            self.held.push(cid);
        }
        Ok(cid)
    }

    /// What is claimed, what judges it, and for each claimed collection
    /// held here, the rule its version names and whether the module is
    /// held: a rule not held defers everything written under it.
    pub(crate) async fn status(&self, db: &DB<RegolithStore>) -> Result<serde_json::Value> {
        let mut rules = Vec::new();
        for name in &self.collections {
            let Some(collection) = db
                .get_collection(name)
                .map_err(|error| WasmError::Storage(error.to_string()))?
            else {
                continue;
            };
            let rule = collection.schema().governance_rule.clone();
            let held = match rule.as_deref().map(str::parse::<Cid>) {
                Some(Ok(cid)) => self
                    .blockstore
                    .has(&cid)
                    .await
                    .map_err(|error| WasmError::Storage(error.to_string()))?,
                _ => false,
            };
            rules.push(serde_json::json!({
                "collection": name,
                "version_id": collection.schema().version_id,
                "rule": rule,
                "held": held,
            }));
        }
        Ok(serde_json::json!({
            "collections": self.collections,
            "engine": self.engine.as_str(),
            "budget": {
                "fuel": self.budget.fuel,
                "steps": self.budget.steps,
                "memory_bytes": self.budget.memory_bytes,
            },
            "modules": self.held.iter().map(Cid::to_string).collect::<Vec<_>>(),
            "rules": rules,
            "sweep": if self.peer_running.load(Ordering::Acquire) { "peer" } else { "local" },
        }))
    }

    /// Stop the sweep and wait for it to let go of the database.
    pub(crate) async fn stop(&mut self) {
        if let Some(sweep) = self.sweep.take() {
            sweep.stop().await;
        }
    }
}

/// Check a module compiles on this client's engine, then hold it.
async fn hold(rules: &WasmRules, modules: &BlockstoreModules<Blocks>, bytes: &[u8]) -> Result<Cid> {
    rules.precompile(bytes).map_err(WasmError::Governance)?;
    modules.put(bytes).await.map_err(WasmError::Governance)
}

/// The governance sweep for a client whose peer is not running.
///
/// A running peer sweeps with its own merge handler, which is the one that
/// fans a re-driven merge out; this one only marks it merged, so it stands
/// aside while a peer runs. Offline, what it can release is a deferral a
/// local write satisfied, a module put since, or anything deferred before
/// the page reloaded.
struct Sweep {
    stop: oneshot::Sender<()>,
    done: oneshot::Receiver<()>,
}

impl Sweep {
    #[allow(clippy::arc_with_non_send_sync)]
    fn spawn(
        db: &Arc<DB<RegolithStore>>,
        blockstore: &Arc<Blocks>,
        peer_running: &Arc<AtomicBool>,
    ) -> Self {
        let handler = Arc::new(DbMergeHandler::new(Arc::clone(db), Arc::clone(blockstore)));
        handler.set_redriven_merge_sink(Arc::new(MarkMergedSink::new(Arc::clone(blockstore))));
        let peer_running = Arc::clone(peer_running);
        let (stop, mut stopped) = oneshot::channel::<()>();
        let (finished, done) = oneshot::channel::<()>();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                if !peer_running.load(Ordering::Acquire) {
                    handler.sweep_unmerged_governed().await;
                }
                let tick = Box::pin(n0_future::time::sleep(SWEEP_INTERVAL));
                // A dropped sender stops the loop too: nothing is left to
                // sweep for once the client is gone.
                if let Either::Right(_) = select(tick, &mut stopped).await {
                    break;
                }
            }
            // The handler holds the database; `close` waits for this.
            drop(handler);
            let _ = finished.send(());
        });
        Self { stop, done }
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.done.await;
    }
}
