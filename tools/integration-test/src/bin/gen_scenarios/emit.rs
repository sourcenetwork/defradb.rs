use integration_test::export::unit_ops::is_unit_op;
use integration_test::export::WireStep;
use std::collections::BTreeSet;

use serde_json::Value;

use super::corpus::Loaded;
use super::text::DEDENTED;
use super::{lifecycle, literal, names, parallel};

const STEP_INDENT: usize = 12;

/// The most actions a test writes in one `vec![..]`.
const CHUNK: usize = 250;

fn is_empty_object(value: &Value) -> bool {
    value.as_object().is_some_and(|m| m.is_empty())
}

/// The Rust types a generated file names, so its imports list exactly those.
#[derive(Default)]
pub struct Used {
    ops: BTreeSet<String>,
    shared: BTreeSet<&'static str>,
    text: bool,
    common: bool,
    chunked: bool,
    /// The names the current test gives its transactions, in the order they begin.
    txns: Vec<String>,
    captures: bool,
    eventually: bool,
    parallel: bool,
}

const SHARED: &[(&str, &str)] = &[
    ("index", "IndexSpec"),
    ("filter", "Filter"),
    ("topics", "Topic"),
    ("expectedAction", "ExpectedAction"),
    ("lens", "Lens"),
    ("eventType", "EventType"),
    ("keyType", "KeyType"),
    ("clientType", "NodeClient"),
    ("indexGate", "IndexGate"),
];

/// Flags that are plain booleans, left out of the arguments when false.
const PLAIN_FLAGS: &[&str] = &["async"];

/// Fields whose value is arbitrary JSON, and those that are a list of it.
const JSON_FIELDS: &[&str] = &["document", "variables", "resultTypes"];
const JSON_LISTS: &[&str] = &["documents", "children"];

/// Fields of the shared types that are optional.
const OPTIONAL_SHARED: &[&str] = &["ordered", "vector", "value", "Lenses"];

/// Fields of the shared types that hold arbitrary JSON.
const JSON_SHARED: &[&str] = &["ordered", "vector", "value", "Arguments"];

fn snake(name: &str) -> String {
    let mut out = String::new();
    let mut previous = ' ';
    for c in name.chars() {
        if c.is_ascii_uppercase() && (previous.is_ascii_lowercase() || previous.is_ascii_digit()) {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
        previous = c;
    }
    out
}

fn shared_type(key: &str) -> Option<&'static str> {
    SHARED.iter().find(|(k, _)| *k == key).map(|(_, t)| *t)
}

/// `Text` for a value that is a reference or a concatenation.
fn text_object(
    map: &serde_json::Map<String, Value>,
    used: &mut Used,
    indent: usize,
) -> Option<String> {
    used.text = true;
    if let (1, Some(Value::String(name))) = (map.len(), map.get("$ref")) {
        return Some(format!("Text::reference({})", literal::string(name)));
    }
    if let (1, Some(Value::Array(parts))) = (map.len(), map.get("$concat")) {
        let items: Vec<String> = parts
            .iter()
            .map(|part| match part {
                Value::String(text) => format!("Text::from({})", literal::string(text)),
                other => value("", other, used, indent),
            })
            .collect();
        return Some(format!("Text::concat([{}])", items.join(", ")));
    }
    None
}

/// A field value as a Rust expression, by the shape of the JSON.
fn value(key: &str, v: &Value, used: &mut Used, indent: usize) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => literal::number(n),
        Value::String(text) => match DEDENTED.contains(&key) && text.contains('\n') {
            true => format!("{}.into()", literal::dedented_string(text, indent)),
            false => format!("{}.into()", literal::string(text)),
        },
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| value(key, item, used, indent))
                .collect();
            format!("vec![{}]", parts.join(", "))
        }
        Value::Object(map) => {
            text_object(map, used, indent).unwrap_or_else(|| literal::json(v, false, indent))
        }
        Value::Null => "None".to_owned(),
    }
}

fn enum_variant(key: &str, v: &Value) -> String {
    let text = v.as_str().unwrap_or_default();
    let variant = match text {
        "JOINED" => "Joined",
        "LEFT" => "Left",
        other => other,
    };
    let ty = shared_type(key).unwrap_or_default();
    format!("{ty}::{variant}")
}

/// A struct literal for a shared type, and for the types nested in it.
fn shared_literal(ty: &str, v: &Value, used: &mut Used, indent: usize) -> String {
    let Some(map) = v.as_object() else {
        return literal::json(v, false, indent);
    };
    used.shared.insert(match ty {
        "IndexField" => "IndexField",
        "LensModule" => "LensModule",
        other => SHARED
            .iter()
            .map(|(_, t)| *t)
            .find(|t| *t == other)
            .unwrap_or("Lens"),
    });
    let inner = " ".repeat(indent + 4);
    let mut out = format!("{ty} {{\n");
    for (key, field) in map {
        let name = snake(key);
        let expr = match (key.as_str(), field) {
            (k, f) if JSON_SHARED.contains(&k) => literal::json(f, false, indent + 4),
            ("fields", Value::Array(items)) => nested_list("IndexField", items, used, indent + 4),
            ("Lenses", Value::Array(items)) => nested_list("LensModule", items, used, indent + 4),
            (k, f) => value(k, f, used, indent + 4),
        };
        let expr = match OPTIONAL_SHARED.contains(&key.as_str()) {
            true => format!("Some({expr})"),
            false => expr,
        };
        out.push_str(&format!("{inner}{name}: {expr},\n"));
    }
    let optional: &[&str] = match ty {
        "IndexSpec" => &["ordered", "vector"],
        "Lens" => &["Lenses"],
        "Filter" => &["value"],
        _ => &[],
    };
    if optional.iter().any(|f| !map.contains_key(*f)) {
        out.push_str(&format!("{inner}..Default::default()\n"));
    }
    out.push_str(&format!("{}}}", " ".repeat(indent)));
    out
}

fn nested_list(ty: &str, items: &[Value], used: &mut Used, indent: usize) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| shared_literal(ty, item, used, indent + 4))
        .collect();
    let pad = " ".repeat(indent + 4);
    match parts.is_empty() {
        true => "vec![]".to_owned(),
        false => format!(
            "vec![\n{}\n{}]",
            parts
                .iter()
                .map(|p| format!("{pad}{p},"))
                .collect::<Vec<_>>()
                .join("\n"),
            " ".repeat(indent)
        ),
    }
}

fn field(key: &str, v: &Value, required: bool, used: &mut Used, indent: usize) -> String {
    let expr = if JSON_FIELDS.contains(&key) {
        literal::json(v, false, indent)
    } else if JSON_LISTS.contains(&key) {
        let items = v.as_array().map(Vec::as_slice).unwrap_or_default();
        let parts: Vec<String> = items
            .iter()
            .map(|item| literal::json(item, false, indent))
            .collect();
        format!("vec![{}]", parts.join(", "))
    } else if let Some(ty) = shared_type(key) {
        used.shared.insert(ty);
        match v {
            Value::Array(items) => nested_list(ty, items, used, indent),
            Value::String(_) => enum_variant(key, v),
            _ => shared_literal(ty, v, used, indent),
        }
    } else {
        value(key, v, used, indent)
    };
    let expr = match required {
        true => expr,
        false => format!("Some({expr})"),
    };
    format!("{}: {expr}", snake(key))
}

/// The Rust field for a corpus argument, where it differs from the name.
fn rust_field(ty: &str, key: &str) -> String {
    match (ty, key) {
        ("Request" | "SubscriptionRequest", "query") => "request".to_owned(),
        (_, "releaseVersion") => "version".to_owned(),
        ("ListActions", "expectedAction") => "after_action".to_owned(),
        (_, "transaction") => "txn".to_owned(),
        (_, "encryptDoc") => "is_doc_encrypted".to_owned(),
        (_, "document") => "doc_map".to_owned(),
        (_, "content") => "import_content".to_owned(),
        (_, "blockCID") => "cid".to_owned(),
        (_, "documents") => "docs".to_owned(),
        (_, "async") => "r#async".to_owned(),
        _ => snake(key),
    }
}

fn txn_index(name: &str, order: &[String]) -> String {
    order
        .iter()
        .position(|n| n == name)
        .unwrap_or(0)
        .to_string()
}

fn strings(items: &[String]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| format!("{}.into()", literal::string(item)))
        .collect();
    format!("vec![{}]", parts.join(", "))
}

/// The `Common` options of a step, if it has any.
fn common_text(step: &WireStep, used: &mut Used, implied_eventually: bool) -> Option<String> {
    let mut parts = Vec::new();
    for (key, v) in step.args.as_object().into_iter().flatten() {
        match key.as_str() {
            "node" if v.as_str() == Some("db") => {}
            "node" => parts.push(format!(
                "node: Some({}.into())",
                literal::string(v.as_str().unwrap_or_default())
            )),
            "identitySeed" => parts.push(format!(
                "identity_seed: Some({})",
                literal::number(v.as_number().expect("seed"))
            )),
            "identityNode" => parts.push(format!(
                "identity_node: Some({}.into())",
                literal::string(v.as_str().unwrap_or_default())
            )),
            _ => {}
        }
    }
    let check = &step.check;
    if step.retry {
        parts.push("retry: true".to_owned());
    }
    if step.envelope {
        parts.push("envelope: true".to_owned());
    }
    if !check.capture.is_empty() {
        used.captures = true;
        let pairs: Vec<String> = check
            .capture
            .iter()
            .map(|(n, p)| format!("({}, {})", literal::string(n), literal::string(p)))
            .collect();
        parts.push(format!("capture: captures([{}])", pairs.join(", ")));
    }
    if let Some(on) = check.compare {
        parts.push(format!("compare: Some({on})"));
    }
    if !check.unordered.is_empty() {
        parts.push(format!("unordered: {}", strings(&check.unordered)));
    }
    if !check.unordered_documents.is_empty() {
        parts.push(format!(
            "unordered_documents: {}",
            strings(&check.unordered_documents)
        ));
    }
    if implied_eventually && check.eventually.is_none() {
        used.eventually = true;
        parts.push("eventually: Some(Eventually::default())".to_owned());
    }
    if let Some(eventually) = &check.eventually {
        used.eventually = true;
        let mut inner = Vec::new();
        if let Some(t) = eventually.timeout_millis {
            inner.push(format!("timeout_millis: Some({t})"));
        }
        if let Some(i) = eventually.interval_millis {
            inner.push(format!("interval_millis: Some({i})"));
        }
        inner.push("..Default::default()".to_owned());
        parts.push(format!(
            "eventually: Some(Eventually {{ {} }})",
            inner.join(", ")
        ));
    }
    if parts.is_empty() {
        return None;
    }
    used.common = true;
    parts.push("..Default::default()".to_owned());
    Some(format!("common: Common {{ {} }}", parts.join(", ")))
}

/// One `name: value` of an action's struct literal, for a corpus argument.
fn arg_field(
    ty: &str,
    key: &str,
    v: &Value,
    plain: bool,
    used: &mut Used,
    indent: usize,
) -> String {
    let text = field(key, v, plain, used, indent);
    let name = snake(key);
    let renamed = rust_field(ty, key);
    match name == renamed {
        true => text,
        false => format!("{renamed}{}", &text[name.len()..]),
    }
}

fn step_text(step: &WireStep, used: &mut Used, depth: usize) -> String {
    let described = integration_test::export::describe(&step.op)
        .unwrap_or_else(|| panic!("no action for op {}", step.op));
    let ty = described.type_name;
    used.ops.insert(ty.to_owned());
    let base = STEP_INDENT + 4 * depth;
    let required: BTreeSet<&str> = described
        .empty
        .as_object()
        .map(|m| m.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let mut fields = Vec::new();
    for (key, v) in step.args.as_object().into_iter().flatten() {
        match key.as_str() {
            "node" | "identitySeed" | "identityNode" => {}
            "transaction" if ty == "TxnBegin" => {}
            "transaction" => {
                let index = txn_index(v.as_str().unwrap_or_default(), &used.txns);
                let expr = match required.contains("txn") {
                    true => index,
                    false => format!("Some({index})"),
                };
                fields.push(format!("txn: {expr}"));
            }
            _ => {
                let plain = required.contains(key.as_str()) || PLAIN_FLAGS.contains(&key.as_str());
                fields.push(arg_field(ty, key, v, plain, used, base + 4));
            }
        }
    }
    let mut implied_eventually = false;
    for (key, v) in &described.implied {
        if key == "eventually" {
            implied_eventually = true;
            continue;
        }
        fields.push(arg_field(ty, key, v, v.is_boolean(), used, base + 4));
    }
    let check = &step.check;
    let graphql = ty == "Request" && !step.envelope;
    let unit = is_unit_op(&step.wire_op()) && check.expect_error.is_none();
    if let Some(expect) = &check.expect {
        if !(unit && is_empty_object(expect)) {
            let data = match graphql {
                true => expect.get("data").unwrap_or(expect),
                false => expect,
            };
            fields.push(format!(
                "{}: {}",
                described.expect_field,
                literal::json(data, false, base + 4)
            ));
            if graphql {
                if let Some(extensions) = expect.get("extensions") {
                    fields.push(format!(
                        "expected_extensions: {}",
                        literal::json(extensions, false, base + 4)
                    ));
                }
            }
        }
    }
    if let Some(message) = &check.expect_error {
        fields.push(format!(
            "expected_error: Some({}.into())",
            literal::string(message)
        ));
    }
    if let Some(common) = common_text(step, used, implied_eventually) {
        fields.push(common);
    }
    fields.push("..Default::default()".to_owned());
    let pad = " ".repeat(base + 4);
    let close = " ".repeat(base);
    let body: Vec<String> = fields
        .iter()
        .map(|f| match f.starts_with("..") {
            true => format!("{pad}{f}"),
            false => format!("{pad}{f},"),
        })
        .collect();
    format!(
        "{close}{ty} {{\n{}\n{close}}}\n{close}.into()",
        body.join("\n")
    )
}

/// One `#[test]` function for the scenario.
pub fn test_fn(loaded: &Loaded, name: &str, leaf: Option<&str>, used: &mut Used) -> String {
    let mut steps = loaded.scenario.steps.clone();
    let without_db = lifecycle::strip_implicit(&mut steps);
    let steps = parallel::expand(steps);
    used.txns = steps
        .iter()
        .filter(|s| s.op == "transaction.begin")
        .filter_map(|s| s.args["transaction"].as_str().map(str::to_owned))
        .collect();
    let mut entries: Vec<String> = Vec::new();
    let mut index = 0;
    while index < steps.len() {
        let end = match steps[index].envelope {
            true => steps[index..]
                .iter()
                .position(|s| !s.envelope)
                .map_or(steps.len(), |n| index + n),
            false => index + 1,
        };
        for step in &steps[index..end] {
            if let Err(reason) = integration_test::export::check_typed(step) {
                panic!("{}: {reason}", loaded.scenario.id);
            }
        }
        let mut entry = String::new();
        if steps[index].envelope {
            used.parallel = true;
            entry.push_str("            Parallel {\n                actions: vec![\n");
            for step in &steps[index..end] {
                entry.push_str(&step_text(step, used, 3));
                entry.push_str(",\n");
            }
            entry.push_str(
                "                ],\n                ..Default::default()\n            }\n            .into(),\n",
            );
        } else {
            entry.push_str(&step_text(&steps[index], used, 0));
            entry.push_str(",\n");
        }
        entries.push(entry);
        index = end;
    }
    let mut out = String::new();
    let mut parts: Vec<String> = Vec::new();
    if entries.len() > CHUNK {
        // A debug build gives every action of a `vec![..]` its own stack
        // slot, so a very long test builds its actions in pieces.
        used.chunked = true;
        for (n, piece) in entries.chunks(CHUNK).enumerate() {
            let helper = format!("{name}_actions_{n}");
            out.push_str(&format!(
                "fn {helper}() -> Vec<ActionInstance<Actions>> {{\n    vec![\n{}    ]\n}}\n\n",
                piece.concat()
            ));
            parts.push(format!("{helper}()"));
        }
    }
    out.push_str("#[test]\n");
    if let Some(leaf) = leaf {
        out.push_str(&format!("#[mantra_macros::req_test({leaf:?})]\n"));
    }
    out.push_str(&format!(
        "fn {name}() {{\n    TestCase {{\n        id: integration_test::scenario_id!(),\n"
    ));
    match parts.is_empty() {
        true => {
            out.push_str("        actions: vec![\n");
            out.push_str(&entries.concat());
            out.push_str("        ],\n");
        }
        false => out.push_str(&format!(
            "        actions: [{}].concat(),\n",
            parts.join(", ")
        )),
    }
    if let Some(reason) = &loaded.scenario.upstream_skip {
        out.push_str(&format!(
            "        skip: Some({}),\n",
            literal::string(reason)
        ));
    }
    if without_db {
        out.push_str("        without_db: true,\n");
    }
    if let Some((kind, reason)) = super::not_applying::lookup(&loaded.scenario.id) {
        out.push_str(&format!(
            "        not_applying: Some(NotApplying::{kind}({})),\n",
            literal::string(reason)
        ));
    }
    out.push_str("        ..Default::default()\n    }\n    .execute();\n}\n");
    out
}

pub fn file_header(loaded: &Loaded, uses_matchers: bool, used: &Used, body: &str) -> String {
    let source = &loaded.scenario.source;
    let mut out = format!(
        "//! Generated from the conformance scenarios of {path} at defradb {revision}.\n\
         //! License: {license}. Copyright {copyright}.\n\n",
        path = source.path,
        revision = &source.revision[..12.min(source.revision.len())],
        license = source.license,
        copyright = source.copyright,
    );
    let mut ops: Vec<&str> = used.ops.iter().map(String::as_str).collect();
    if used.parallel {
        ops.push("Parallel");
    }
    if used.chunked {
        ops.push("Actions");
    }
    ops.sort_unstable();
    let shared: Vec<&str> = used.shared.iter().copied().collect();
    if !ops.is_empty() {
        out.push_str(&format!(
            "use integration_test::actions::{{{}}};\n",
            ops.join(", ")
        ));
    }
    if !shared.is_empty() {
        out.push_str(&format!(
            "use integration_test::actions::shared::{{{}}};\n",
            shared.join(", ")
        ));
    }
    if uses_matchers {
        out.push_str("use integration_test::m;\n");
    }
    let mut scenario_uses = Vec::new();
    if used.common {
        scenario_uses.push("Common");
    }
    if used.captures {
        scenario_uses.push("captures");
    }
    if used.eventually {
        scenario_uses.push("Eventually");
    }
    if used.text {
        scenario_uses.push("Text");
    }
    if !scenario_uses.is_empty() {
        scenario_uses.sort_unstable();
        out.push_str(&format!(
            "use integration_test::scenario::{{{}}};\n",
            scenario_uses.join(", ")
        ));
    }
    match body.contains("NotApplying::") {
        true => out.push_str("use integration_test::test_case::{NotApplying, TestCase};\n"),
        false => out.push_str("use integration_test::test_case::TestCase;\n"),
    }
    if used.chunked {
        out.push_str("use testo::action::ActionInstance;\n");
    }
    if body.contains("json!") {
        out.push_str("use serde_json::json;\n");
    }
    out.push('\n');
    out
}

pub fn function_name(loaded: &Loaded) -> String {
    names::test_fn(loaded)
}
