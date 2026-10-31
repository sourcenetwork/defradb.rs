//! An action as the scenario step it exports to, and back.

use serde_json::{json, Map, Value};

use super::wire::{Check, WireStep};
use crate::scenario::number::json_eq;
use crate::scenario::Op;

/// A test on the fields an action serializes to.
pub type Test = fn(&Map<String, Value>) -> bool;

/// One corpus op an action can stand for. An action with options is several
/// corpus ops: `AddDoc { is_doc_encrypted: Some(true), .. }` is
/// `document.createEncrypted`. A variant says which fields tell it apart, the
/// fields its name implies (kept out of the step's arguments), and the fields
/// the corpus calls something else.
#[derive(Clone, Debug)]
pub struct Variant {
    pub op: &'static str,
    /// Fields that must all be set, by serialized name; a dotted name looks
    /// inside an object.
    pub all: &'static [&'static str],
    /// When not empty, at least one of these must be set.
    pub any: &'static [&'static str],
    /// Fields the corpus op's name implies, as `(name, JSON value)`.
    pub implied: &'static [(&'static str, &'static str)],
    /// Fields the corpus spells differently, as `(action's name, step's name)`.
    pub renames: &'static [(&'static str, &'static str)],
    /// A last test on the fields, when no field decides it.
    pub when: Option<Test>,
}

/// The variant for a corpus op named `op`, that matches whatever fields are set.
pub fn v(op: &'static str) -> Variant {
    Variant {
        op,
        all: &[],
        any: &[],
        implied: &[],
        renames: &[],
        when: None,
    }
}

impl Variant {
    pub fn all(mut self, keys: &'static [&'static str]) -> Self {
        self.all = keys;
        self
    }

    pub fn any(mut self, keys: &'static [&'static str]) -> Self {
        self.any = keys;
        self
    }

    pub fn implied(mut self, fields: &'static [(&'static str, &'static str)]) -> Self {
        self.implied = fields;
        self
    }

    pub fn renames(mut self, fields: &'static [(&'static str, &'static str)]) -> Self {
        self.renames = fields;
        self
    }

    pub fn when(mut self, test: Test) -> Self {
        self.when = Some(test);
        self
    }

    fn matches(&self, fields: &Map<String, Value>) -> bool {
        let set = |path: &&str| {
            // `name=json` also compares the value.
            let (path, wanted) = match path.split_once('=') {
                Some((path, json)) => (path, serde_json::from_str::<Value>(json).ok()),
                None => (*path, None),
            };
            let mut parts = path.split('.');
            let first = parts.next().unwrap_or_default();
            let mut current = fields.get(first);
            for part in parts {
                current = current.and_then(|v| v.get(part));
            }
            match wanted {
                Some(wanted) => current == Some(&wanted),
                None => current.is_some_and(|v| !v.is_null()),
            }
        };
        self.all.iter().all(&set)
            && (self.any.is_empty() || self.any.iter().any(&set))
            && self.when.is_none_or(|test| test(fields))
    }
}

/// Whether every child of a group is an op that needs no transaction: the
/// corpus calls such a group `action.parallel` and any other `document.parallel`.
pub fn all_safe(fields: &Map<String, Value>) -> bool {
    const SAFE: &[&str] = &[
        "document.createWithRetry",
        "query.execute",
        "collection.truncate",
    ];
    fields
        .get("children")
        .and_then(Value::as_array)
        .is_some_and(|children| {
            children.iter().all(|child| {
                child["op"].as_str().is_some_and(|op| SAFE.contains(&op))
                    && child["args"].get("transaction").is_none()
            })
        })
}

/// The name of transaction number `index` in a scenario.
pub fn txn_name(index: u64) -> String {
    format!("txn{index}")
}

fn txn_index(name: &str) -> Option<u64> {
    name.strip_prefix("txn")?.parse().ok()
}

/// The scenario step an action exports to.
pub fn to_wire<A>(action: &A, variants: &[Variant]) -> WireStep
where
    A: Op,
{
    let mut fields = match serde_json::to_value(action) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let variant = variants
        .iter()
        .find(|v| v.matches(&fields))
        .or(variants.last())
        .expect("an action has a corpus op");
    for (name, _) in variant.implied {
        fields.remove(*name);
    }
    for (from, to) in variant.renames {
        if let Some(value) = fields.remove(*from) {
            fields.insert((*to).to_owned(), value);
        }
    }
    let op = variant.op;
    let expected = fields.remove("expectedData");
    let extensions = fields.remove("expectedExtensions");
    let expect_error = fields.remove("expectedError");
    let retry = fields.remove("retry").is_some();
    let envelope = fields.remove("envelope").is_some();
    let capture = fields.remove("capture");
    let compare = fields.remove("compare").and_then(|v| v.as_bool());
    let unordered = fields.remove("unordered");
    let unordered_documents = fields.remove("unorderedDocuments");
    let eventually = fields.remove("eventually");
    if let Some(index) = fields.remove("txn").and_then(|v| v.as_u64()) {
        fields.insert("transaction".to_owned(), json!(txn_name(index)));
    }
    let strings = |value: Option<Value>| -> Vec<String> {
        serde_json::from_value(value.unwrap_or(Value::Null)).unwrap_or_default()
    };
    let expect = expected.map(|data| match A::GRAPHQL && !envelope {
        true => {
            let mut out = json!({"data": data});
            if let Some(extensions) = extensions {
                out["extensions"] = extensions;
            }
            out
        }
        false => data,
    });
    WireStep {
        op: op.to_owned(),
        args: Value::Object(fields),
        check: Check {
            expect,
            expect_error: expect_error.and_then(|v| v.as_str().map(str::to_owned)),
            capture: serde_json::from_value(capture.unwrap_or(Value::Null)).unwrap_or_default(),
            unordered: strings(unordered),
            unordered_documents: strings(unordered_documents),
            compare,
            eventually: eventually.and_then(|v| serde_json::from_value(v).ok()),
        },
        retry,
        envelope,
    }
}

/// The action a scenario step is, if the action holds everything the step
/// says.
pub fn from_wire<A>(step: &WireStep, variants: &[Variant]) -> Result<A, String>
where
    A: Op,
{
    let op = step.op.as_str();
    let variant = variants
        .iter()
        .find(|v| v.op == op)
        .ok_or_else(|| format!("{op} is not one of this action's ops"))?;
    let mut fields = match &step.args {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    if let Some(name) = fields.remove("transaction") {
        if op != "transaction.begin" {
            let index = name.as_str().and_then(txn_index);
            fields.insert("txn".to_owned(), json!(index));
        }
    }
    for (name, value) in variant.implied {
        let value: Value = serde_json::from_str(value).unwrap_or(Value::Null);
        fields.insert((*name).to_owned(), value);
    }
    let check = &step.check;
    if let Some(expect) = &check.expect {
        let data = match A::GRAPHQL && !step.envelope {
            true => {
                if let Some(extensions) = expect.get("extensions") {
                    fields.insert("expectedExtensions".to_owned(), extensions.clone());
                }
                expect.get("data").cloned().unwrap_or(Value::Null)
            }
            false => expect.clone(),
        };
        fields.insert("expectedData".to_owned(), data);
    }
    if let Some(message) = &check.expect_error {
        fields.insert("expectedError".to_owned(), json!(message));
    }
    if step.retry {
        fields.insert("retry".to_owned(), json!(true));
    }
    if step.envelope {
        fields.insert("envelope".to_owned(), json!(true));
    }
    if !check.capture.is_empty() {
        fields.insert("capture".to_owned(), json!(check.capture));
    }
    if let Some(compare) = check.compare {
        fields.insert("compare".to_owned(), json!(compare));
    }
    if !check.unordered.is_empty() {
        fields.insert("unordered".to_owned(), json!(check.unordered));
    }
    if !check.unordered_documents.is_empty() {
        fields.insert(
            "unorderedDocuments".to_owned(),
            json!(check.unordered_documents),
        );
    }
    if let Some(eventually) = &check.eventually {
        fields.insert("eventually".to_owned(), json!(eventually));
    }
    for (from, to) in variant.renames {
        if let Some(value) = fields.remove(*to) {
            fields.insert((*from).to_owned(), value);
        }
    }
    let action: A =
        serde_json::from_value(Value::Object(fields)).map_err(|e| format!("{op}: {e}"))?;
    let mut again = to_wire(&action, variants);
    if op == "transaction.begin" {
        if let (Some(args), Some(name)) = (again.args.as_object_mut(), step.args.get("transaction"))
        {
            args.insert("transaction".to_owned(), name.clone());
        }
    }
    let same = again.op == step.op
        && again.retry == step.retry
        && again.envelope == step.envelope
        && again.check == step.check
        && json_eq(&again.args, &step.args);
    match same {
        true => Ok(action),
        false => Err(format!(
            "{} does not round trip: {} became {}",
            op,
            serde_json::to_string(step).unwrap_or_default(),
            serde_json::to_string(&again).unwrap_or_default()
        )),
    }
}
