//! The two checks graphql-go runs over an input value: the literal rule
//! validation runs on what the request wrote (`isValidLiteralValue`), and the
//! coercion that runs on a variable's value (`isValidInputValue`).

use serde_json::Value as JsonValue;

use crate::mapper::{PathStep, VariablePosition};

use super::shape::{Collections, Field, Shape};

/// The variables a literal names, below the value being walked.
pub(super) struct Skips<'v> {
    positions: &'v [VariablePosition],
}

impl<'v> Skips<'v> {
    pub(super) fn new(positions: &'v [VariablePosition]) -> Self {
        Self { positions }
    }

    fn at(&self, path: &[PathStep]) -> bool {
        self.positions.iter().any(|position| position.path == path)
    }
}

/// graphql-go's literal check of `value` against `shape`. A variable is
/// assumed valid here; coercion checks it. Unknown keys come first, in the
/// order written, then each defined key's messages; a list element's
/// messages are numbered by their place among that element's messages, as
/// graphql-go numbers them.
pub(super) fn literal(
    collections: &Collections<'_>,
    shape: Shape<'_>,
    value: &JsonValue,
    path: &mut Vec<PathStep>,
    skips: &Skips<'_>,
) -> Vec<String> {
    if skips.at(path) {
        return Vec::new();
    }
    let JsonValue::Object(fields) = value else {
        return Vec::new();
    };
    literal_fields(collections, shape, fields, path, skips)
}

/// [`literal`] for an object value's fields.
pub(super) fn literal_fields(
    collections: &Collections<'_>,
    shape: Shape<'_>,
    fields: &serde_json::Map<String, JsonValue>,
    path: &mut Vec<PathStep>,
    skips: &Skips<'_>,
) -> Vec<String> {
    let mut messages: Vec<String> = fields
        .keys()
        .filter(|key| matches!(collections.field(shape, key), Field::Unknown))
        .map(|key| format!("In field \"{key}\": Unknown field."))
        .collect();
    for (key, field_value) in sorted(fields) {
        path.push(PathStep::Key(key.clone()));
        let nested = match collections.field(shape, key) {
            Field::Unknown => Vec::new(),
            Field::One(inner) => literal(collections, inner, field_value, path, skips),
            Field::ListOf(inner) => literal_list(collections, inner, field_value, path, skips),
        };
        path.pop();
        messages.extend(
            nested
                .into_iter()
                .map(|message| format!("In field \"{key}\": {message}")),
        );
    }
    messages
}

fn literal_list(
    collections: &Collections<'_>,
    shape: Shape<'_>,
    value: &JsonValue,
    path: &mut Vec<PathStep>,
    skips: &Skips<'_>,
) -> Vec<String> {
    if skips.at(path) {
        return Vec::new();
    }
    let JsonValue::Array(items) = value else {
        return literal(collections, shape, value, path, skips);
    };
    let mut messages = Vec::new();
    for (index, item) in items.iter().enumerate() {
        path.push(PathStep::Index(index));
        let nested = literal(collections, shape, item, path, skips);
        path.pop();
        messages.extend(
            nested
                .into_iter()
                .enumerate()
                .map(|(number, message)| format!("In element #{}: {message}", number + 1)),
        );
    }
    messages
}

/// graphql-go's coercion check of a variable's `value` against `shape`:
/// unknown keys first, then each defined key's messages, both by sorted key,
/// and list elements numbered by position.
pub(super) fn variable(
    collections: &Collections<'_>,
    shape: Shape<'_>,
    value: &JsonValue,
) -> Vec<String> {
    let JsonValue::Object(fields) = value else {
        return Vec::new();
    };
    variable_fields(collections, shape, fields)
}

/// [`variable`] for an object value's fields.
pub(super) fn variable_fields(
    collections: &Collections<'_>,
    shape: Shape<'_>,
    fields: &serde_json::Map<String, JsonValue>,
) -> Vec<String> {
    let sorted = sorted(fields);
    let mut messages: Vec<String> = sorted
        .iter()
        .filter(|(key, _)| matches!(collections.field(shape, key), Field::Unknown))
        .map(|(key, _)| format!("In field \"{key}\": Unknown field."))
        .collect();
    for (key, field_value) in sorted {
        let nested = match collections.field(shape, key) {
            Field::Unknown => Vec::new(),
            Field::One(inner) => variable(collections, inner, field_value),
            Field::ListOf(inner) => match field_value {
                JsonValue::Array(items) => items
                    .iter()
                    .enumerate()
                    .flat_map(|(index, item)| {
                        variable(collections, inner, item)
                            .into_iter()
                            .map(move |message| format!("In element #{}: {message}", index + 1))
                    })
                    .collect(),
                other => variable(collections, inner, other),
            },
        };
        messages.extend(
            nested
                .into_iter()
                .map(|message| format!("In field \"{key}\": {message}")),
        );
    }
    messages
}

/// The shape at `path` below `shape`, or `None` where the path leaves every
/// generated input type.
pub(super) fn shape_at<'a>(
    collections: &Collections<'a>,
    mut shape: Shape<'a>,
    path: &[PathStep],
) -> Option<Shape<'a>> {
    let mut in_list = false;
    for step in path {
        match step {
            PathStep::Index(_) if in_list => in_list = false,
            PathStep::Index(_) => return None,
            PathStep::Key(key) => {
                if in_list {
                    in_list = false;
                }
                match collections.field(shape, key) {
                    Field::Unknown => return None,
                    Field::One(inner) => shape = inner,
                    Field::ListOf(inner) => {
                        shape = inner;
                        in_list = true;
                    }
                }
            }
        }
    }
    (!in_list).then_some(shape)
}

fn sorted(fields: &serde_json::Map<String, JsonValue>) -> Vec<(&String, &JsonValue)> {
    let mut sorted: Vec<_> = fields.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    sorted
}
