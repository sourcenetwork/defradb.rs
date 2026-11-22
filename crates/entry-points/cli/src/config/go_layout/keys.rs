//! The Go keys spelled differently from their Rust fields, and the Go keys
//! Rust has no setting for.

use serde_yaml::{Mapping, Value};

use super::set_default;

/// Each Go key as viper writes it (`section.key`, or a root key) and the
/// Rust key it fills. `proofs/lean/GoConfigLayout.lean` holds the same table.
const KEYS: &[(&str, &str)] = &[
    ("net.p2pdisabled", "net.p2p_disabled"),
    ("net.p2paddresses", "net.p2p_addresses"),
    ("net.pubsubenabled", "net.pubsub_enabled"),
    ("net.relay", "net.relay_enabled"),
    ("api.pubkeypath", "api.pubkey_path"),
    ("api.privkeypath", "api.privkey_path"),
    ("api.allowed-origins", "api.allowed_origins"),
    ("datastore.maxtxnretries", "datastore.max_txn_retries"),
    ("datastore.noencryption", "datastore.no_encryption"),
    (
        "datastore.nosearchableencryption",
        "datastore.no_searchable_encryption",
    ),
    ("datastore.nosigning", "datastore.no_signing"),
    ("datastore.defaultkeytype", "datastore.default_key_type"),
    ("log.colordisabled", "log.color_disabled"),
    ("secretfile", "secret_file"),
    ("telemetry.disabled", "telemetry_disabled"),
    ("replicator.retryintervals", "replicator_retry_intervals"),
];

/// Go's default for a key Rust has no setting for.
enum GoDefault {
    Str(&'static str),
    Bool(bool),
    Int(i64),
}

impl GoDefault {
    fn is(&self, value: &Value) -> bool {
        match self {
            Self::Str(s) => value.as_str() == Some(*s),
            Self::Bool(b) => value.as_bool() == Some(*b),
            Self::Int(i) => value.as_i64() == Some(*i),
        }
    }
}

/// Go keys with no Rust setting, and Go's default for each (cli/config/config.go
/// ConfigDefaults). Go's `datastore.badger.valuelogfilesize` and viper's `""`
/// rootdir key are not listed: serde ignores them, and they say nothing an
/// operator chose for this node.
const GO_ONLY: &[(&str, GoDefault)] = &[
    ("api.audience", GoDefault::Str("")),
    ("log.caller", GoDefault::Bool(false)),
    ("net.p2pblocksynctimeout", GoDefault::Int(30)),
];

/// Moves every Go key in `root`, whose sections' keys are lowercased, onto its
/// Rust key, unless the Rust key is already there, and pushes a warning onto
/// `warnings` for each Go-only key set to something other than Go's default.
pub(super) fn apply(root: &mut Mapping, warnings: &mut Vec<String>) {
    for (go, rust) in KEYS {
        if let Some(value) = take(root, go) {
            put_default(root, rust, value);
        }
    }
    for (go, default) in GO_ONLY {
        if let Some(value) = take(root, go) {
            if !default.is(&value) {
                warnings.push(format!("{go} is ignored: defradb.rs has no such setting"));
            }
        }
    }
}

fn take(root: &mut Mapping, path: &str) -> Option<Value> {
    match path.split_once('.') {
        Some((section, key)) => root.get_mut(section)?.as_mapping_mut()?.remove(key),
        None => root.remove(path),
    }
}

fn put_default(root: &mut Mapping, path: &str, value: Value) {
    let Some((section, key)) = path.split_once('.') else {
        return set_default(root, path, value);
    };
    let section = root
        .entry(section.into())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if let Value::Mapping(section) = section {
        set_default(section, key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::KEYS;

    /// The table the Lean proof is about is this one, row for row.
    #[test]
    fn the_table_matches_the_lean_model() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../proofs/lean/GoConfigLayout.lean"
        );
        let lean = std::fs::read_to_string(path).expect("read GoConfigLayout.lean");
        let body = lean
            .split("def table : List (String × String) := [")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("GoConfigLayout.lean defines `table`");
        let rows: Vec<(String, String)> = body
            .lines()
            .filter_map(|line| {
                let line = line.trim().trim_end_matches(',');
                let inner = line.strip_prefix('(')?.strip_suffix(')')?;
                let (go, rust) = inner.split_once(", ")?;
                Some((go.trim_matches('"').into(), rust.trim_matches('"').into()))
            })
            .collect();
        let rust: Vec<(String, String)> = KEYS
            .iter()
            .map(|(go, rust)| ((*go).into(), (*rust).into()))
            .collect();
        assert_eq!(rows, rust);
    }
}
