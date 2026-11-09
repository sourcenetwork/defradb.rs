//! `sameSchema`: a GraphQL introspection result describes at least the golden
//! schema. Everything the golden has must be there and match; what only the
//! result has (a type, field or argument defradb.rs adds) is allowed.
//!
//! Introspection lists types, fields, arguments and enum values in whatever
//! order the server builds them (Go's follows map iteration), so lists compare
//! as sets keyed by `name`. A mismatch prints the differing paths, a missing
//! subtree once at its top, since the whole result runs to hundreds of
//! kilobytes. Descriptions compare by their words, so reflowed text, `""` and
//! `null` match.

use std::collections::BTreeMap;

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
    let differing: Vec<String> = wanted
        .iter()
        .filter_map(|(path, (w, parent))| match found.get(path) {
            Some((g, _)) if g != w && !same_description(path, w, g) => {
                Some(format!("  ~ {path}: want {w}, got {g}"))
            }
            None if found.contains_key(parent) || parent.is_empty() => {
                Some(format!("  - {path}: {w}"))
            }
            _ => None,
        })
        .collect();
    if differing.is_empty() {
        return true;
    }
    eprintln!(
        "sameSchema: {} differences (- missing, ~ changed), first {}:\n{}",
        differing.len(),
        SHOWN.min(differing.len()),
        differing[..SHOWN.min(differing.len())].join("\n")
    );
    false
}

/// Two descriptions say the same thing when their words match: SDL reflows a
/// description's indentation and blank lines, and an empty description and
/// none at all are the same (Go reports `""` where async-graphql reports
/// `null`).
fn same_description(path: &str, want: &str, got: &str) -> bool {
    let words = |text: &str| {
        serde_json::from_str::<Option<String>>(text)
            .ok()
            .flatten()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    path.ends_with(".description") && words(want) == words(got)
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
