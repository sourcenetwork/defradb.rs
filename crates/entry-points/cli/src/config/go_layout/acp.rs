//! Go DefraDB's nested `acp.document.*` keys, moved onto Rust's flat `acp` keys.

use serde_yaml::{Mapping, Value};

use super::{lowercase_keys, set_default};

/// Go key under `acp.document.remote` and the Rust `acp` key it fills.
const REMOTE_KEYS: &[(&str, &str)] = &[("address", "remote_address")];

/// Go keys under `acp.document.remote` that configure Go's chain client,
/// which this implementation does not have.
const IGNORED_REMOTE_KEYS: &[&str] = &["keyname", "logid", "grpcaddress", "cometrpcaddress"];

/// Moves Go's `document.type` and `document.remote.*` onto Rust's keys in
/// `root`'s `acp` section, whose keys are already lowercased.
/// Each of [`IGNORED_REMOTE_KEYS`] is dropped, with a warning pushed onto
/// `warnings`.
pub(super) fn apply(root: &mut Mapping, warnings: &mut Vec<String>) {
    let Some(Value::Mapping(acp)) = root.get_mut("acp") else {
        return;
    };
    let Some(Value::Mapping(document)) = acp.remove("document") else {
        return;
    };
    let mut document = lowercase_keys(document);
    if let Some(kind) = document.remove("type") {
        set_default(acp, "document_type", kind);
    }
    if let Some(Value::Mapping(remote)) = document.remove("remote") {
        let mut remote = lowercase_keys(remote);
        for key in IGNORED_REMOTE_KEYS {
            if remote.remove(*key).is_some() {
                warnings.push(format!(
                    "acp.document.remote.{key} is ignored: Go's remote document ACP client is not supported"
                ));
            }
        }
        for (go, rust) in REMOTE_KEYS {
            if let Some(value) = remote.remove(*go) {
                set_default(acp, rust, value);
            }
        }
    }
}
