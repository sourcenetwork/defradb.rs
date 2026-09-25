//! Merge governance for a browser node: the collections its application
//! claims, judged by the wasm rule module each version names, on the same
//! engine a relay can run.

mod config;
#[cfg(target_arch = "wasm32")]
mod runtime;

pub use config::{BudgetConfig, GovernanceConfig};
#[cfg(target_arch = "wasm32")]
pub(crate) use runtime::Governance;
