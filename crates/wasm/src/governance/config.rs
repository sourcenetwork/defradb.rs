//! The `governance` key of the client's `create` config.

use db::merge::governance::rule::{RuleBudget, RuleEngine};
use serde_bytes::ByteBuf;

use crate::error::{Result, WasmError};

/// Collections this client's application claims, judged by the wasm rule
/// module each version names.
///
/// ```javascript
/// governance: {
///   collections: ['Move', 'Turn'],
///   rule_modules: [moduleBytes],          // Uint8Array each, optional
///   budget: { fuel, steps, memory_bytes }, // each optional
///   engine: 'wasmi',                      // optional; the only one here
/// }
/// ```
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GovernanceConfig {
    pub collections: Vec<String>,
    /// Module bytes to hold before anything is judged, so a version naming
    /// one of them is judged at once rather than deferred until it arrives.
    pub rule_modules: Vec<ByteBuf>,
    pub budget: BudgetConfig,
    pub engine: Option<String>,
}

/// A [`RuleBudget`], each bound defaulting to [`RuleBudget::default`]'s.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BudgetConfig {
    pub fuel: Option<u64>,
    pub steps: Option<usize>,
    pub memory_bytes: Option<usize>,
}

impl GovernanceConfig {
    /// The collections, engine and budget this config asks for, refused
    /// when it claims nothing or names an engine this build cannot run: a
    /// governance that quietly governs nothing is the misconfiguration
    /// this exists to prevent.
    pub fn resolve(&self) -> Result<(Vec<String>, RuleEngine, RuleBudget)> {
        let mut collections: Vec<String> = self
            .collections
            .iter()
            .map(|name| name.trim().to_string())
            .collect();
        if let Some(empty) = collections.iter().position(String::is_empty) {
            return Err(WasmError::Governance(format!(
                "governance.collections[{empty}] is empty"
            )));
        }
        collections.sort();
        collections.dedup();
        if collections.is_empty() {
            return Err(WasmError::Governance(
                "governance claims no collections".to_string(),
            ));
        }
        let engine = match self.engine.as_deref() {
            None => RuleEngine::default(),
            Some(name) => name.parse().map_err(WasmError::Governance)?,
        };
        if !engine.is_available() {
            return Err(WasmError::Governance(format!(
                "rule engine {engine} is not available in this build"
            )));
        }
        Ok((collections, engine, self.budget.resolve()?))
    }
}

impl BudgetConfig {
    pub fn resolve(&self) -> Result<RuleBudget> {
        let defaults = RuleBudget::default();
        let budget = RuleBudget {
            fuel: self.fuel.unwrap_or(defaults.fuel),
            steps: self.steps.unwrap_or(defaults.steps),
            memory_bytes: self.memory_bytes.unwrap_or(defaults.memory_bytes),
        };
        if budget.fuel == 0 || budget.steps == 0 || budget.memory_bytes == 0 {
            return Err(WasmError::Governance(
                "a governance budget of zero admits no verdict".to_string(),
            ));
        }
        Ok(budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> GovernanceConfig {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn collections_are_trimmed_sorted_and_deduplicated() {
        let (collections, engine, budget) = parse(r#"{"collections": [" Turn", "Move", "Turn"]}"#)
            .resolve()
            .unwrap();
        assert_eq!(collections, ["Move", "Turn"]);
        assert_eq!(engine, RuleEngine::default());
        assert_eq!(budget, RuleBudget::default());
    }

    #[test]
    fn claiming_nothing_is_refused() {
        assert!(parse(r#"{}"#).resolve().is_err());
        assert!(parse(r#"{"collections": ["Move", " "]}"#)
            .resolve()
            .is_err());
    }

    #[test]
    fn a_budget_overrides_only_what_it_names() {
        let (_, _, budget) = parse(r#"{"collections": ["Move"], "budget": {"fuel": 5000}}"#)
            .resolve()
            .unwrap();
        assert_eq!(
            budget,
            RuleBudget {
                fuel: 5000,
                ..RuleBudget::default()
            }
        );
        assert!(
            parse(r#"{"collections": ["Move"], "budget": {"steps": 0}}"#)
                .resolve()
                .is_err()
        );
    }

    #[test]
    fn modules_arrive_as_bytes_and_unknown_keys_are_refused() {
        let config = parse(r#"{"collections": ["Move"], "rule_modules": [[0, 97, 115, 109]]}"#);
        assert_eq!(config.rule_modules[0].as_ref(), b"\0asm");
        assert!(serde_json::from_str::<GovernanceConfig>(
            r#"{"collections": ["Move"], "rules": []}"#
        )
        .is_err());
    }

    #[test]
    fn an_engine_is_named_or_refused() {
        let (_, engine, _) = parse(r#"{"collections": ["Move"], "engine": "wasmi"}"#)
            .resolve()
            .unwrap();
        assert_eq!(engine, RuleEngine::Wasmi);
        assert!(parse(r#"{"collections": ["Move"], "engine": "v8"}"#)
            .resolve()
            .is_err());
    }
}
