//! The GraphQL text of each document write.
//!
//! This is the trust boundary: caller JSON becomes GraphQL syntax only here.
//! Names written unquoted (the collection, object keys, encrypted fields)
//! must be GraphQL `Name`s, so none can close the argument list and append
//! operations of its own; every other value is a JSON-encoded literal.

use super::{AddOptions, Error, Filter, Result};
use serde_json::Value;

const SELECTION: &str = "{ _docID }";

/// A GraphQL `Name`: `/[_A-Za-z][_0-9A-Za-z]*/` (GraphQL spec 2.1.9).
fn name(name: &str) -> Result<&str> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric());
    match valid {
        true => Ok(name),
        false => Err(Error::Request(format!("'{name}' is not a valid name"))),
    }
}

fn string(text: &str) -> String {
    serde_json::to_string(text).expect("a string always serializes")
}

/// `value` as a GraphQL input literal. Strings use JSON's encoding, which
/// is a valid GraphQL `StringValue` and what Go's test harness writes.
pub(super) fn input(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => string(s),
        Value::Array(items) => {
            let items = items.iter().map(input).collect::<Result<Vec<_>>>()?;
            format!("[{}]", items.join(", "))
        }
        Value::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(key, value)| Ok(format!("{}: {}", name(key)?, input(value)?)))
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", fields.join(", "))
        }
    })
}

fn encryption(options: &AddOptions) -> Result<String> {
    let mut args = String::new();
    if options.encrypt_doc {
        args.push_str(", encrypt: true");
    }
    if !options.encrypted_fields.is_empty() {
        let fields = options
            .encrypted_fields
            .iter()
            .map(|field| name(field))
            .collect::<Result<Vec<_>>>()?;
        args.push_str(&format!(", encryptFields: [{}]", fields.join(", ")));
    }
    Ok(args)
}

pub(super) fn add(collection: &str, docs: &[Value], options: &AddOptions) -> Result<String> {
    let docs = docs.iter().map(input).collect::<Result<Vec<_>>>()?;
    Ok(format!(
        "mutation {{ add_{}(input: [{}]{}) {SELECTION} }}",
        name(collection)?,
        docs.join(", "),
        encryption(options)?
    ))
}

pub(super) fn update(collection: &str, doc_id: &str, patch: &Value) -> Result<String> {
    Ok(format!(
        "mutation {{ update_{}(docIDs: [{}], input: {}) {SELECTION} }}",
        name(collection)?,
        string(doc_id),
        input(patch)?
    ))
}

pub(super) fn save(
    collection: &str,
    doc_id: &str,
    doc: &Value,
    options: &AddOptions,
) -> Result<String> {
    let doc = input(doc)?;
    Ok(format!(
        "mutation {{ upsert_{}(filter: {{_docID: {{_eq: {}}}}}, add: {doc}, update: {doc}{}) {SELECTION} }}",
        name(collection)?,
        string(doc_id),
        encryption(options)?
    ))
}

pub(super) fn delete(collection: &str, doc_id: &str) -> Result<String> {
    Ok(format!(
        "mutation {{ delete_{}(docIDs: [{}]) {SELECTION} }}",
        name(collection)?,
        string(doc_id)
    ))
}

/// The filter as GraphQL input. Source text is parsed into conditions
/// first, so it is checked exactly like a filter object.
fn filter(filter: &Filter) -> Result<String> {
    match filter {
        Filter::Conditions(conditions) => input(&Value::Object(conditions.clone())),
        Filter::Source(source) if source.trim().is_empty() => {
            Err(Error::Request("filter cannot be empty".to_owned()))
        }
        Filter::Source(source) => crate::parse_filter_string(source)
            .map_err(|e| Error::Request(e.to_string()))
            .and_then(|conditions| input(&conditions)),
        Filter::Unsupported(_) => Err(Error::Request("unsupported filter type".to_owned())),
    }
}

pub(super) fn update_with_filter(
    collection: &str,
    filter_value: &Filter,
    updater: &str,
) -> Result<String> {
    let collection = name(collection)?;
    let filter = filter(filter_value)?;
    let updater: Value = serde_json::from_str(updater).map_err(|e| {
        let what = if updater.trim_start().starts_with('{') {
            "cannot parse object: "
        } else {
            ""
        };
        Error::Request(format!("cannot parse JSON: {what}{e}"))
    })?;
    if !updater.is_object() {
        return Err(Error::Request(
            "the updater of a document is of invalid type".to_owned(),
        ));
    }
    Ok(format!(
        "mutation {{ update_{collection}(filter: {filter}, input: {}) {SELECTION} }}",
        input(&updater)?
    ))
}

pub(super) fn delete_with_filter(collection: &str, filter_value: &Filter) -> Result<String> {
    Ok(format!(
        "mutation {{ delete_{}(filter: {}) {SELECTION} }}",
        name(collection)?,
        filter(filter_value)?
    ))
}
