//! Reads a `config.yaml` Go DefraDB wrote into Rust's keys.
//!
//! Go writes its config through viper, which lowercases every key, so a Go key
//! spelled differently from its Rust field would be silently ignored. Section
//! names are matched case-insensitively and each section's keys lowercased, as
//! viper reads them; then every Go key is moved onto its Rust key, and a Rust
//! key already present wins (`proofs/lean/GoConfigLayout.lean`).

mod acp;
mod keys;

use serde_yaml::{Mapping, Value};

/// Rewrites a parsed `config.yaml` so Go's keys land on Rust's, and returns one
/// warning for each Go key it drops that the operator set. They are returned,
/// not logged, because no subscriber is installed until the config is loaded.
pub(super) fn apply(root: &mut Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let Some(root) = root.as_mapping_mut() else {
        return warnings;
    };
    *root = lowercase_keys(std::mem::take(root))
        .into_iter()
        .map(|(key, value)| match value {
            Value::Mapping(section) => (key, Value::Mapping(lowercase_keys(section))),
            value => (key, value),
        })
        .collect();
    acp::apply(root, &mut warnings);
    keys::apply(root, &mut warnings);
    warnings
}

fn lowercase_keys(map: Mapping) -> Mapping {
    map.into_iter()
        .map(|(k, v)| match k {
            Value::String(s) => (Value::String(s.to_ascii_lowercase()), v),
            k => (k, v),
        })
        .collect()
}

fn set_default(map: &mut Mapping, key: &str, value: Value) {
    if !map.contains_key(key) {
        map.insert(key.into(), value);
    }
}
