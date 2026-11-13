//! A lens module argument's value as a module block stores it, and back.
//!
//! Go's lens store (`lens/host-go/store/block.go` `writeConfigBlock`) writes
//! each argument as `json.Marshal` of its value. Go decodes a lens config with
//! `UseNumber` at every entry point, so a number is written as the text it
//! arrived as, and a nested object's keys come out sorted. Both runtimes must
//! write the same bytes for the same lens, or its ID differs between them.

use serde_json::{Map, Value};

/// `value` as Go's lens store writes it into a module block: JSON with every
/// object's keys sorted and the characters Go's `json.Marshal` escapes and
/// serde_json does not (`<`, `>`, `&`, U+2028, U+2029) escaped.
pub fn encode_lens_argument(value: &Value) -> String {
    go_json_escape(&sorted(value).to_string())
}

/// A stored argument back to its value. A block written before arguments were
/// JSON-encoded holds a string argument raw, so a value that is not JSON is
/// that string.
pub fn decode_lens_argument(stored: &str) -> Value {
    serde_json::from_str(stored).unwrap_or_else(|_| Value::String(stored.to_owned()))
}

/// `value` with every object rebuilt in key order, which is the order it
/// serializes in whether or not serde_json's `preserve_order` is enabled.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// `json` with Go's extra escapes. The escaped characters appear in JSON only
/// inside strings, where the escape means the same.
fn go_json_escape(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}
