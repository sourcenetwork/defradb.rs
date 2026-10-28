use serde_json::{Number, Value};

/// The number's value as `<digits without trailing zeros>e<exponent>`, so
/// `1`, `1.0` and `10e-1` are equal and nothing passes through a float.
pub fn canonical_number(number: &Number) -> String {
    let text = number.to_string().to_lowercase();
    let (mantissa, exponent) = match text.split_once('e') {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (text.as_str(), None),
    };
    let mut power: i128 = match exponent {
        Some(exponent) => match exponent.parse() {
            Ok(power) => power,
            Err(_) => return text,
        },
        None => 0,
    };
    let negative = mantissa.starts_with('-');
    let mantissa = mantissa.trim_start_matches('-');
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let joined = format!("{whole}{fraction}");
    let digits = joined.trim_start_matches('0');
    if digits.is_empty() {
        return "0".to_owned();
    }
    let trimmed = digits.trim_end_matches('0');
    power += (digits.len() - trimmed.len()) as i128 - fraction.len() as i128;
    let sign = if negative { "-" } else { "" };
    format!("{sign}{trimmed}e{power}")
}

/// JSON equality with exact decimal numbers. Strings never equal numbers and
/// `null` never equals a missing key.
pub fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => canonical_number(x) == canonical_number(y),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, p)| y.get(key).is_some_and(|q| json_eq(p, q)))
        }
        _ => a == b,
    }
}

/// Canonical JSON text, used to order arrays: sorted keys, canonical numbers.
pub fn canonical_text(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Number(number) => out.push_str(&canonical_number(number)),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}
