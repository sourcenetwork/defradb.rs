use query::documents;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A document filter: its kind, and the filter body for the kinds that have one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

impl Filter {
    /// The filter as the core takes it. `none` is Go's absent filter, which
    /// matches every document; `structNumber` is a Go struct, which no
    /// client accepts.
    pub fn to_core(&self) -> documents::Filter {
        let value = self.value.clone().unwrap_or(Value::Null);
        match self.kind.as_str() {
            "none" => documents::Filter::Conditions(serde_json::Map::new()),
            "structNumber" => documents::Filter::Unsupported(value),
            _ => value.into(),
        }
    }

    /// The filter as the CLI's `--filter` takes it: GraphQL source text. A
    /// filter of no kind Go knows cannot be written as text, so the CLI
    /// refuses it with the core's words.
    pub fn to_cli(&self) -> Result<String, documents::Error> {
        match self.to_core() {
            documents::Filter::Conditions(conditions) => Ok(graphql(&Value::Object(conditions))),
            documents::Filter::Source(source) => Ok(source),
            documents::Filter::Unsupported(_) => Err(documents::Error::Request(
                "unsupported filter type".to_owned(),
            )),
        }
    }
}

/// `value` as a GraphQL input literal: object keys bare, every other value
/// in its JSON form.
fn graphql(value: &Value) -> String {
    match value {
        Value::Object(fields) => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(key, value)| format!("{key}: {}", graphql(value)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(graphql).collect();
            format!("[{}]", items.join(", "))
        }
        other => other.to_string(),
    }
}
