//! Cursor pagination with `_cursor` (`defra.cursor`).

mod composite_index;
mod errors;
mod filter_index;
mod in_filter;
mod page_info;
mod paging;
mod reindex;
mod seek;
mod token;

use integration_test::actions::{Actions, AddCollection, AddDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use serde_json::{json, Value};

const USERS: &str = "type User { name: String age: Int @index }";

fn collection(sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

fn doc(value: Value) -> Actions {
    doc_in("User", value)
}

fn doc_in(collection: &str, value: Value) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(value),
        ..Default::default()
    }
    .into()
}

/// `sdl`'s `User` holding one document per age, named after it.
fn aged(sdl: &str, ages: &[i64]) -> Vec<Actions> {
    let mut actions = vec![collection(sdl)];
    actions.extend(
        ages.iter()
            .map(|age| doc(json!({ "name": format!("u{age}"), "age": age }))),
    );
    actions
}

/// A `_cursor` page of `User` rows with `args` and the whole page info;
/// `after` names a captured token or holds a literal one.
fn page(args: &str, after: Option<Text>, fields: &str, expected: Value) -> Request {
    let tail =
        format!("{{ {fields} }} _pageInfo {{ hasNext hasPrev startCursor endCursor }} }} }}");
    let request = match after {
        Some(token) => Text::concat([
            Text::from(format!("query {{ _cursor {{ User({args}, after: \"")),
            token,
            Text::from(format!("\") {tail}")),
        ]),
        None => Text::from(format!("query {{ _cursor {{ User({args}) {tail}")),
    };
    Request {
        request,
        expected_data: expected,
        ..Default::default()
    }
}

/// `head`, the captured token `token`, then `tail`, as one request text.
fn around(head: &str, token: &str, tail: &str) -> Text {
    Text::concat([Text::from(head), Text::reference(token), Text::from(tail)])
}

/// `request` expecting `expected` as its data.
fn query(request: impl Into<Text>, expected: Value) -> Request {
    Request {
        request: request.into(),
        expected_data: expected,
        ..Default::default()
    }
}

/// `request` failing with an error containing `error`.
fn fails(request: &str, error: &str) -> Actions {
    Request {
        request: request.into(),
        expected_error: Some(error.into()),
        ..Default::default()
    }
    .into()
}

/// `request`, capturing its end cursor as `name`.
fn capture_end(request: Request, name: &str) -> Actions {
    Request {
        common: Common {
            capture: captures([(name, "/data/_cursor/_pageInfo/endCursor")]),
            ..Default::default()
        },
        ..request
    }
    .into()
}

fn ages(ages: &[i64]) -> Value {
    Value::Array(ages.iter().map(|age| json!({ "age": age })).collect())
}

fn names(names: &[&str]) -> Value {
    Value::Array(names.iter().map(|name| json!({ "name": name })).collect())
}

/// `rows` in any order.
fn unordered(rows: Value) -> Value {
    json!({ "$match": "unordered", "items": rows })
}

/// One row per binding, `field` holding that binding's value: the first
/// request to see a binding fixes it, every later one must match it.
fn same(field: &str, bindings: &[&str]) -> Value {
    Value::Array(
        bindings
            .iter()
            .map(|name| json!({ field: { "$match": "same", "name": name } }))
            .collect(),
    )
}

/// A page of `rows` with `hasNext`/`hasPrev` and non-empty cursors.
fn rows(rows: Value, has_next: bool, has_prev: bool) -> Value {
    let token = json!({ "$match": "nonemptyString" });
    json!({ "_cursor": {
        "User": rows,
        "_pageInfo": { "hasNext": has_next, "hasPrev": has_prev, "startCursor": token, "endCursor": token },
    } })
}

/// A `_cursor` result of `collection`'s `rows` and no page info.
fn only(collection: &str, rows: Value) -> Value {
    json!({ "_cursor": { collection: rows } })
}
