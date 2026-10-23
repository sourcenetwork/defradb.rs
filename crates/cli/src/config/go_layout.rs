//! Reads Go DefraDB's nested `acp.document.*` YAML keys into Rust's flat `acp` keys.

use serde_yaml::{Mapping, Value};

/// Go key under `acp.document.remote` and the Rust `acp` key it fills.
const REMOTE_KEYS: &[(&str, &str)] = &[
    ("logid", "vera_chain_id"),
    ("grpcaddress", "vera_grpc_address"),
    ("cometrpcaddress", "vera_comet_address"),
    ("address", "remote_address"),
];

/// Lowercases the keys of `acp`, as viper does, then moves Go's
/// `document.type` and `document.remote.*` onto Rust's keys.
/// A Rust key that is already present wins over its Go spelling.
/// Go's `document.remote.keyname` has no Rust equivalent and is dropped.
pub(super) fn apply(root: &mut Value) {
    let Some(root) = root.as_mapping_mut() else {
        return;
    };
    let Some(acp) = take_ci(root, "acp") else {
        return;
    };
    let Value::Mapping(acp) = acp else {
        root.insert("acp".into(), acp);
        return;
    };
    let mut acp = lowercase_keys(acp);
    if let Some(Value::Mapping(document)) = acp.remove("document") {
        let mut document = lowercase_keys(document);
        if let Some(kind) = document.remove("type") {
            set_default(&mut acp, "document_type", kind);
        }
        if let Some(Value::Mapping(remote)) = document.remove("remote") {
            let mut remote = lowercase_keys(remote);
            for (go, rust) in REMOTE_KEYS {
                if let Some(value) = remote.remove(*go) {
                    set_default(&mut acp, rust, value);
                }
            }
        }
    }
    root.insert("acp".into(), Value::Mapping(acp));
}

fn take_ci(map: &mut Mapping, name: &str) -> Option<Value> {
    let key = map
        .keys()
        .find(|k| k.as_str().is_some_and(|k| k.eq_ignore_ascii_case(name)))?
        .clone();
    map.remove(&key)
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
