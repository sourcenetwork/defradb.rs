//! Cursor pagination with `_cursor` (`defra.cursor`).

mod in_filter;
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
    AddDoc {
        collection: "User".into(),
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

/// A page of `rows` with `hasNext`/`hasPrev` and non-empty cursors.
fn rows(rows: Value, has_next: bool, has_prev: bool) -> Value {
    let token = json!({ "$match": "nonemptyString" });
    json!({ "_cursor": {
        "User": rows,
        "_pageInfo": { "hasNext": has_next, "hasPrev": has_prev, "startCursor": token, "endCursor": token },
    } })
}
