//! Merge governance for a `defra start` node: installed on the database
//! before any network surface starts, so no merge and no local write into a
//! claimed collection happens ungoverned.

use std::sync::Arc;

use db::merge::governance::rule::{BlockstoreModules, RuleBudget, WasmRules};
use db::merge::governance::{install_merge_governance, MergeGovernance};
use tracing::info;

use super::node::Node;
use crate::config::{Config, GovernanceConfig};
use crate::error::{Error, Result};

impl Node {
    /// Claim `config.governance.collections` for a [`WasmRules`] validator
    /// reading modules from this node's blockstore, holding each configured
    /// module first, and judge this node's own writes by it.
    ///
    /// Installed whether or not P2P runs: a relay started with `--no-p2p`
    /// merges nothing, but its HTTP mutations are local writes, and those
    /// are judged only when a judge is installed.
    pub(super) async fn install_governance(
        database: &Arc<db::DB<storage::DynStore>>,
        store: &Arc<storage::DynStore>,
        config: &Config,
    ) -> Result<()> {
        let governance = &config.governance;
        if governance.collections.is_empty() {
            if governance.rule_modules.is_empty() {
                return Ok(());
            }
            return Err(Error::InvalidConfig(
                "rule modules were given but no collection is governed (--governed)".into(),
            ));
        }
        let engine = governance.rule_engine.into();
        let blockstore = Arc::new(blockstore::DefraBlockstore::new(store.clone(), true));
        let rules = WasmRules::with_engine(
            Arc::new(BlockstoreModules::new(blockstore.clone())),
            engine,
            budget(governance)?,
        )
        .map_err(|error| Error::InvalidConfig(format!("rule engine {engine}: {error}")))?;
        let modules = BlockstoreModules::new(blockstore.clone());
        for path in &governance.rule_modules {
            let bytes = std::fs::read(path)
                .map_err(|error| Error::InvalidConfig(format!("rule module {path}: {error}")))?;
            let cid = rules
                .precompile(&bytes)
                .map_err(|error| Error::InvalidConfig(format!("rule module {path}: {error}")))?;
            modules
                .put(&bytes)
                .await
                .map_err(|error| Error::InvalidConfig(format!("rule module {path}: {error}")))?;
            info!(path = %path, cid = %cid, "Rule module held");
        }
        install_merge_governance(
            database,
            blockstore,
            MergeGovernance::new(governance.collections.iter().cloned())
                .with_validator(Arc::new(rules)),
            config.datastore.max_merge_depth,
        );
        info!(
            collections = ?governance.collections,
            engine = %engine,
            "Merge governance installed"
        );
        Ok(())
    }
}

/// The configured budget, each bound defaulting to [`RuleBudget::default`]'s.
/// A zero bound admits no verdict, so every write would stay unmerged.
fn budget(governance: &GovernanceConfig) -> Result<RuleBudget> {
    let defaults = RuleBudget::default();
    let budget = RuleBudget {
        fuel: governance.fuel.unwrap_or(defaults.fuel),
        steps: governance.steps.unwrap_or(defaults.steps),
        memory_bytes: governance.memory_bytes.unwrap_or(defaults.memory_bytes),
    };
    if budget.fuel == 0 || budget.steps == 0 || budget.memory_bytes == 0 {
        return Err(Error::InvalidConfig(
            "a governance budget of zero admits no verdict".into(),
        ));
    }
    Ok(budget)
}
