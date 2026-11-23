//! The two forms graphql-go prints a bad input value in: the argument as
//! written, and a variable's value as Go's `encoding/json` marshals it.

use serde_json::Value as JsonValue;

use crate::mapper::{PathStep, VariablePosition};

use super::super::filters::push_quoted;

/// A literal filter as graphql-go's printer shows it, with each variable it
/// names back in place as `$name`. graphql_parser holds an object's keys
/// sorted, so keys print sorted where Go prints them as written.
pub(super) fn literal(
    fields: &serde_json::Map<String, JsonValue>,
    variables: &[VariablePosition],
) -> String {
    let mut out = String::new();
    let mut path = Vec::new();
    print_object(fields, variables, &mut path, &mut out);
    out
}

fn print_literal(
    value: &JsonValue,
    variables: &[VariablePosition],
    path: &mut Vec<PathStep>,
    out: &mut String,
) {
    if let Some(position) = variables.iter().find(|position| position.path == *path) {
        out.push('$');
        out.push_str(&position.name);
        return;
    }
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Number(n) => out.push_str(&n.to_string()),
        JsonValue::String(s) => push_quoted(s, out),
        JsonValue::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                path.push(PathStep::Index(index));
                print_literal(item, variables, path, out);
                path.pop();
            }
            out.push(']');
        }
        JsonValue::Object(fields) => print_object(fields, variables, path, out),
    }
}

fn print_object(
    fields: &serde_json::Map<String, JsonValue>,
    variables: &[VariablePosition],
    path: &mut Vec<PathStep>,
    out: &mut String,
) {
    out.push('{');
    for (index, (key, field)) in fields.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(key);
        out.push_str(": ");
        path.push(PathStep::Key(key.clone()));
        print_literal(field, variables, path, out);
        path.pop();
    }
    out.push('}');
}

/// `value` as Go's `json.Marshal` writes a decoded variable: compact, object
/// keys sorted, and `<`, `>`, `&`, U+2028 and U+2029 escaped.
pub(crate) fn go_json(value: &JsonValue) -> String {
    let mut out = String::new();
    print_go_json(value, &mut out);
    out
}

fn print_go_json(value: &JsonValue, out: &mut String) {
    match value {
        JsonValue::Object(fields) => {
            let mut sorted: Vec<_> = fields.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            out.push('{');
            for (index, (key, field)) in sorted.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                push_go_string(key, out);
                out.push(':');
                print_go_json(field, out);
            }
            out.push('}');
        }
        JsonValue::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                print_go_json(item, out);
            }
            out.push(']');
        }
        JsonValue::String(s) => push_go_string(s, out),
        other => out.push_str(&other.to_string()),
    }
}

fn push_go_string(s: &str, out: &mut String) {
    let encoded = serde_json::to_string(s).unwrap_or_default();
    for c in encoded.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
}
