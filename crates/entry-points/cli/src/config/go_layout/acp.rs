//! Go DefraDB's nested `acp.document.*` keys, moved onto Rust's flat `acp` keys.

use serde_yaml::{Mapping, Value};

use super::{lowercase_keys, set_default};

/// Go key under `acp.document.remote` and the Rust `acp` key it fills.
const REMOTE_KEYS: &[(&str, &str)] = &[
    ("logid", "vera_chain_id"),
    ("grpcaddress", "vera_grpc_address"),
    ("cometrpcaddress", "vera_comet_address"),
    ("address", "remote_address"),
];

/// Moves Go's `document.type` and `document.remote.*` onto Rust's keys in
/// `root`'s `acp` section, whose keys are already lowercased.
/// Go's `document.remote.keyname` has no Rust equivalent: it is dropped, with a
/// warning pushed onto `warnings`.
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
        if remote.remove("keyname").is_some() {
            warnings.push(
                "acp.document.remote.keyname is ignored: the Vera signer comes from --identity"
                    .into(),
            );
        }
        for (go, rust) in REMOTE_KEYS {
            if let Some(value) = remote.remove(*go) {
                set_default(acp, rust, value);
            }
        }
    }
}
