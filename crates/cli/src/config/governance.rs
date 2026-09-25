use serde::{Deserialize, Serialize};

use super::types::RuleEngineType;

/// Merge governance: the collections this node's application claims, whose
/// merges and local writes are judged by the wasm rule module each version
/// names (`@governed(rule:)`).
///
/// Unset by default, and left out of a written config file while unset, so
/// a node that governs nothing behaves and serialises as before.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct GovernanceConfig {
    /// Collection names claimed.
    pub collections: Vec<String>,
    /// Rule module files held at startup, relative to the root directory
    /// unless absolute.
    pub rule_modules: Vec<String>,
    /// The engine modules run on.
    pub rule_engine: RuleEngineType,
    /// Fuel per step, in the engine's own units. `None`: the default budget's.
    pub fuel: Option<u64>,
    /// Steps per verdict. `None`: the default budget's.
    pub steps: Option<usize>,
    /// Linear memory a module may grow to, in bytes. `None`: the default
    /// budget's.
    pub memory_bytes: Option<usize>,
}

impl GovernanceConfig {
    pub fn is_unset(&self) -> bool {
        self == &Self::default()
    }
}
