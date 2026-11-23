//! Filter parsing helpers
//!
//! Standalone functions for parsing GraphQL filter arguments:
//! - `parse_filter_value()` - Parse a filter argument into a Filter
//! - `print_object_value()` - Print an argument as graphql-go's errors do

use graphql_parser::query::Value;
use rapidhash::RapidHashMap;
use serde_json::{Map, Value as JsonValue};

use crate::error::{QueryError, Result};
use crate::mapper::{Filter, FilterSource, PathStep, VariablePosition};

use super::values::{graphql_value_to_json, graphql_value_to_json_no_vars};

/// Parse a filter argument value into a Filter, recording whether the request
/// wrote it or a variable supplied it.
pub(super) fn parse_filter_value(
    value: &Value<'_, String>,
    variables: Option<&RapidHashMap<String, JsonValue>>,
) -> Result<Filter> {
    match value {
        Value::Object(obj) => {
            let mut positions = Vec::new();
            let conditions = parse_filter_object(obj, variables, &mut positions)?;
            Ok(Filter::from_conditions(conditions).with_source(FilterSource::literal(positions)))
        }
        Value::Variable(name) => {
            let vars = variables.ok_or_else(|| {
                QueryError::parse(format!("Variable \"${}\" was not provided.", name))
            })?;
            let json_val = vars.get(name.as_str()).ok_or_else(|| {
                QueryError::parse(format!("Variable \"${}\" was not provided", name))
            })?;
            if let JsonValue::Object(obj) = json_val {
                Ok(Filter::from_conditions(obj.clone())
                    .with_source(FilterSource::Variable(name.clone())))
            } else {
                Err(QueryError::parse("filter must be an object"))
            }
        }
        _ => Err(QueryError::parse("filter must be an object")),
    }
}

/// Parse a filter object into conditions map, noting in `positions` where
/// each variable it names sits.
fn parse_filter_object(
    obj: &std::collections::BTreeMap<String, Value<'_, String>>,
    variables: Option<&RapidHashMap<String, JsonValue>>,
    positions: &mut Vec<VariablePosition>,
) -> Result<Map<String, JsonValue>> {
    let mut conditions = Map::new();
    let mut path = Vec::new();

    for (key, val) in obj {
        path.push(PathStep::Key(key.clone()));
        note_variables(val, &mut path, positions);
        path.pop();
        let json_val = graphql_value_to_json(val, variables)?;
        conditions.insert(key.clone(), json_val);
    }

    Ok(conditions)
}

/// Record the position of every variable inside `value`, which sits at `path`.
fn note_variables(
    value: &Value<'_, String>,
    path: &mut Vec<PathStep>,
    positions: &mut Vec<VariablePosition>,
) {
    match value {
        Value::Variable(name) => positions.push(VariablePosition {
            path: path.clone(),
            name: name.clone(),
        }),
        Value::Object(fields) => {
            for (key, field) in fields {
                path.push(PathStep::Key(key.clone()));
                note_variables(field, path, positions);
                path.pop();
            }
        }
        Value::List(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(PathStep::Index(index));
                note_variables(item, path, positions);
                path.pop();
            }
        }
        _ => {}
    }
}

fn print_value(value: &Value<'_, String>, out: &mut String) {
    match value {
        Value::Variable(name) => {
            out.push('$');
            out.push_str(name);
        }
        Value::Int(n) => {
            if let Some(n) = n.as_i64() {
                out.push_str(&n.to_string());
            }
        }
        Value::Float(f) => match serde_json::Number::from_f64(*f) {
            Some(n) => out.push_str(&n.to_string()),
            None => out.push_str(&f.to_string()),
        },
        Value::String(s) => push_quoted(s, out),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::Enum(name) => out.push_str(name),
        Value::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                print_value(item, out);
            }
            out.push(']');
        }
        Value::Object(fields) => print_object(fields, out),
    }
}

/// [`print_argument_value`] for an object argument's fields.
pub(super) fn print_object_value(
    fields: &std::collections::BTreeMap<String, Value<'_, String>>,
) -> String {
    let mut out = String::new();
    print_object(fields, &mut out);
    out
}

fn print_object(fields: &std::collections::BTreeMap<String, Value<'_, String>>, out: &mut String) {
    out.push('{');
    for (index, (key, field)) in fields.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(key);
        out.push_str(": ");
        print_value(field, out);
    }
    out.push('}');
}

/// A string as graphql-go prints a string literal (Go's `strconv.Quote`,
/// for the printable characters a query holds).
pub(crate) fn push_quoted(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse Go's string filter form into filter conditions.
///
/// Go types a filter as `any` and accepts GraphQL source for it, which its own
/// client and the JS client both send (`internal/db/document_update.go:168-176`
/// via `NewFilterFromString`). An empty string is `ErrEmptyFilter` there, so it
/// is an error here too rather than a match-all.
///
/// Parsed rather than pasted: the source is wrapped in a throwaway operation
/// and run through the real parser, so what comes back is a JSON object that
/// goes on to be validated like any other filter. Splicing the caller's text
/// into the mutation would reopen exactly the hole key validation closes.
pub fn parse_filter_string(source: &str) -> Result<JsonValue> {
    if source.trim().is_empty() {
        return Err(QueryError::parse("filter cannot be empty"));
    }

    let document = format!("query {{ probe(filter: {source}) {{ _docID }} }}");
    let parsed = graphql_parser::parse_query::<String>(&document)
        .map_err(|e| QueryError::parse(format!("invalid filter: {e}")))?;

    let filter = parsed
        .definitions
        .iter()
        .find_map(|definition| match definition {
            graphql_parser::query::Definition::Operation(
                graphql_parser::query::OperationDefinition::Query(query),
            ) => query
                .selection_set
                .items
                .iter()
                .find_map(|item| match item {
                    graphql_parser::query::Selection::Field(field) => field
                        .arguments
                        .iter()
                        .find(|(name, _)| name == "filter")
                        .map(|(_, value)| value),
                    _ => None,
                }),
            _ => None,
        })
        .ok_or_else(|| QueryError::parse("invalid filter"))?;

    match graphql_value_to_json_no_vars(filter)? {
        JsonValue::Object(conditions) => Ok(JsonValue::Object(conditions)),
        _ => Err(QueryError::parse("filter must be an object")),
    }
}
