use serde_json::{Number, Value};

use super::text::{dedent, raw_safe, DEDENTED};

const WIDTH: usize = 88;

fn raw(text: &str) -> String {
    let mut hashes = 1;
    while text.contains(&format!("\"{}", "#".repeat(hashes))) {
        hashes += 1;
    }
    let marks = "#".repeat(hashes);
    format!("r{marks}\"{text}\"{marks}")
}

/// A Rust string literal with exactly the bytes of `text`.
pub fn string(text: &str) -> String {
    let plain = !text.contains(['"', '\\', '\n', '\t']) && raw_safe(text);
    if plain {
        return format!("\"{text}\"");
    }
    if raw_safe(text) && text.contains(['\n', '"', '\\']) {
        return raw(text);
    }
    format!("{text:?}")
}

/// A multi-line string, dedented, and indented to sit in a test body.
pub fn dedented_string(text: &str, indent: usize) -> String {
    if !raw_safe(text) {
        return format!("{text:?}");
    }
    let body = dedent(text)
        .split('\n')
        .map(|line| match line.is_empty() {
            true => String::new(),
            false => format!("{}{line}", " ".repeat(indent + 4)),
        })
        .collect::<Vec<_>>()
        .join("\n");
    raw(&format!("\n{body}\n{}", " ".repeat(indent)))
}

pub fn number(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        return match i32::try_from(i) {
            Ok(_) => i.to_string(),
            Err(_) => format!("{i}_i64"),
        };
    }
    if let Some(u) = n.as_u64() {
        return format!("{u}_u64");
    }
    let text = n.to_string();
    match text.contains(['.', 'e', 'E']) {
        true => text,
        false => format!("{text}.0"),
    }
}

fn json_macro(value: &Value, top_level_args: bool, indent: usize) -> String {
    format!("json!({})", body(value, top_level_args, indent))
}

fn matcher(map: &serde_json::Map<String, Value>, indent: usize) -> Option<String> {
    let kind = map.get("$match")?.as_str()?;
    let json = |key: &str| Some(json_macro(map.get(key)?, false, indent));
    let text = |key: &str| Some(string(map.get(key)?.as_str()?));
    let call = match kind {
        "object" => format!("m::object({})", json("fields")?),
        "optional" => format!("m::optional({})", json("value")?),
        "unordered" => format!("m::unordered({})", json("items")?),
        "contains" => format!("m::contains({})", json("items")?),
        "all" => format!("m::all({})", json("of")?),
        "anyOf" => format!("m::any_of({})", json("of")?),
        "same" => format!("m::same({})", text("name")?),
        "unique" => format!("m::unique({})", text("name")?),
        "cid" => "m::cid()".to_owned(),
        "docID" => "m::doc_id()".to_owned(),
        "nonempty" => "m::nonempty()".to_owned(),
        "nonemptyString" => "m::nonempty_string()".to_owned(),
        "currentTimestamp" => "m::current_timestamp()".to_owned(),
        "introspectionTypes" => "m::introspection_types()".to_owned(),
        "length" => format!("m::length({})", map.get("value")?.as_u64()?),
        "substring" => format!("m::substring({})", text("value")?),
        "sortedStrings" => format!("m::sorted_strings({})", text("field")?),
        "firstWhere" => format!(
            "m::first_where({}, {}, {})",
            text("field")?,
            text("value")?,
            json("expect")?
        ),
        "similarityScore" => format!(
            "m::similarity_score({}, {}, {})",
            text("metric")?,
            json("source")?,
            json("vector")?
        ),
        "notPlainCBORString" => format!("m::not_plain_cbor_string({})", json("value")?),
        "explainScan" => match map.contains_key("path") {
            true => format!(
                "m::explain_scan_at({}, {})",
                json("metrics")?,
                json("path")?
            ),
            false => format!("m::explain_scan({})", json("metrics")?),
        },
        "explainPattern" => format!("m::explain_pattern({})", json("pattern")?),
        "explainTarget" => format!(
            "m::explain_target({}, {}, {}, {})",
            text("name")?,
            map.get("includeChildren")?.as_bool()?,
            map.get("skip")?.as_u64()?,
            json("attributes")?
        ),
        "branchableCreateGraph" => format!("m::branchable_create_graph({})", text("name")?),
        _ => return None,
    };
    Some(call)
}

fn concat(parts: &[Value], indent: usize) -> String {
    let items: Vec<String> = parts
        .iter()
        .map(|part| match part {
            Value::String(text) => format!("json!({})", string(text)),
            other => body(other, false, indent),
        })
        .collect();
    format!("m::concat([{}])", items.join(", "))
}

/// The value as the body of a `json!` call. Matchers and references become
/// `m::` calls; `args` marks the top-level args object, whose SDL and query
/// strings are dedented.
pub fn body(value: &Value, args: bool, indent: usize) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number(n),
        Value::String(text) => string(text),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(|v| body(v, false, indent + 4)).collect();
            wrap('[', ']', &parts, indent)
        }
        Value::Object(map) => {
            if let Some(call) = matcher(map, indent) {
                return call;
            }
            if let (1, Some(Value::String(name))) = (map.len(), map.get("$ref")) {
                return format!("m::r({})", string(name));
            }
            if let (1, Some(Value::Array(parts))) = (map.len(), map.get("$concat")) {
                return concat(parts, indent);
            }
            let parts: Vec<String> = map
                .iter()
                .map(|(key, child)| {
                    let rendered = match (child, args && DEDENTED.contains(&key.as_str())) {
                        (Value::String(text), true) if text.contains('\n') => {
                            dedented_string(text, indent + 4)
                        }
                        _ => body(child, false, indent + 4),
                    };
                    format!("{key:?}: {rendered}")
                })
                .collect();
            wrap('{', '}', &parts, indent)
        }
    }
}

fn wrap(open: char, close: char, parts: &[String], indent: usize) -> String {
    if parts.is_empty() {
        return format!("{open}{close}");
    }
    let flat = parts.join(", ");
    let fits = indent + flat.len() + 2 <= WIDTH && !flat.contains('\n');
    if fits {
        return match open {
            '{' => format!("{{ {flat} }}"),
            _ => format!("{open}{flat}{close}"),
        };
    }
    let pad = " ".repeat(indent + 4);
    let lines: Vec<String> = parts.iter().map(|p| format!("{pad}{p},")).collect();
    format!(
        "{open}\n{}\n{}{close}",
        lines.join("\n"),
        " ".repeat(indent)
    )
}

/// `json!(..)` for an args or expectation value.
pub fn json(value: &Value, args: bool, indent: usize) -> String {
    json_macro(value, args, indent)
}
