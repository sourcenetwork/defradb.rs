//! `sameSchema`: two GraphQL introspection results describe the same schema.
//!
//! Introspection lists types, fields, arguments and enum values in whatever
//! order the server builds them (Go's follows map iteration), so lists compare
//! as sets keyed by `name`. A mismatch prints the differing paths, a missing or
//! extra subtree once at its top, since the whole result runs to hundreds of
//! kilobytes.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// How many differing paths a mismatch prints.
const SHOWN: usize = 60;

/// Every node of a value by path: its leaf text (or a marker for an object or
/// list) and the path of the node that holds it.
type Nodes = BTreeMap<String, (String, String)>;

pub(super) fn same_schema(want: &Value, got: &Value) -> bool {
    let (mut wanted, mut found) = (Nodes::new(), Nodes::new());
    flatten(want, String::new(), String::new(), &mut wanted);
    flatten(got, String::new(), String::new(), &mut found);
    if wanted == found {
        return true;
    }
    let only = |path: &str, side: &Nodes, other: &Nodes| {
        side.contains_key(path) && !other.contains_key(path)
    };
    let differing: Vec<String> = wanted
        .keys()
        .chain(found.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|path| match (wanted.get(path), found.get(path)) {
            (Some((w, _)), Some((g, _))) if w != g => {
                Some(format!("  ~ {path}: want {w}, got {g}"))
            }
            (Some((w, parent)), None) if !only(parent, &wanted, &found) => {
                Some(format!("  - {path}: {w}"))
            }
            (None, Some((g, parent))) if !only(parent, &found, &wanted) => {
                Some(format!("  + {path}: {g}"))
            }
            _ => None,
        })
        .collect();
    eprintln!(
        "sameSchema: {} differences (- missing, + extra, ~ changed), first {}:\n{}",
        differing.len(),
        SHOWN.min(differing.len()),
        differing[..SHOWN.min(differing.len())].join("\n")
    );
    false
}

/// Records every node of `value` under a path in which list items are named by
/// their `name` when they have one, so the order of a list does not matter.
fn flatten(value: &Value, path: String, parent: String, out: &mut Nodes) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                flatten(child, format!("{path}.{key}"), path.clone(), out);
            }
            out.insert(path, ("{…}".to_owned(), parent));
        }
        Value::Array(items) => {
            for item in items {
                let key = match item.get("name").and_then(Value::as_str) {
                    Some(name) => format!("[{name}]"),
                    None => format!("[{item}]"),
                };
                flatten(item, format!("{path}{key}"), path.clone(), out);
            }
            out.insert(path, ("[…]".to_owned(), parent));
        }
        leaf => {
            out.insert(path, (leaf.to_string(), parent));
        }
    }
}
