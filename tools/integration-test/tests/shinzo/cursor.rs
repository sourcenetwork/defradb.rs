//! Cursor pagination with `_cursor` (`go.shinzo.cursor`), against a Rust
//! node in-process and a Go Shinzo node under `DEFRA_CLIENT=go-shinzo`.
//! Tokens are compared byte for byte: DocIDs are content-addressed, so a
//! row's token is the same on either node.

use integration_test::actions::{Actions, AddCollection, AddDoc, DeleteDoc, Request};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const USERS: &str = "type User { name: String age: Int @index }";

fn collection(sdl: &str) -> Actions {
    AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    }
    .into()
}

fn doc(value: Value, capture: Option<&str>) -> Actions {
    AddDoc {
        collection: "User".into(),
        doc_map: Some(value),
        common: Common {
            capture: capture.map_or_else(Default::default, |name| captures([(name, "/docID")])),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// `User` documents named after their ages, `User` holding `age: Int @index`.
fn aged(ages: &[i64]) -> Vec<Actions> {
    let mut actions = vec![collection(USERS)];
    actions.extend(ages.iter().map(|age| {
        doc(
            json!({ "name": format!("u{age}"), "age": age }),
            Some(&format!("u{age}")),
        )
    }));
    actions
}

/// A `_cursor` page of `User` rows with `args`, its rows' `fields` and the
/// whole page info; `after`/`before` name a captured token.
fn page(args: &str, cursor: Option<(&str, &str)>, fields: &str, expected: Value) -> Request {
    let head = match cursor {
        Some((arg, _)) => format!("query {{ _cursor {{ User({args}, {arg}: \""),
        None => format!("query {{ _cursor {{ User({args}) {{ {fields} }} _pageInfo {{ hasNext hasPrev startCursor endCursor }} }} }}"),
    };
    let request = match cursor {
        Some((_, token)) => Text::concat([
            Text::from(head),
            Text::reference(token),
            Text::from(format!(
                "\") {{ {fields} }} _pageInfo {{ hasNext hasPrev startCursor endCursor }} }} }}"
            )),
        ]),
        None => Text::from(head),
    };
    Request {
        request,
        expected_data: expected,
        ..Default::default()
    }
}

/// `page`, capturing its end and start cursors as `<name>_end` and
/// `<name>_start`.
fn captured(request: Request, name: &str) -> Actions {
    let end = format!("{name}_end");
    let start = format!("{name}_start");
    Request {
        common: Common {
            capture: captures([
                (end.as_str(), "/data/_cursor/_pageInfo/endCursor"),
                (start.as_str(), "/data/_cursor/_pageInfo/startCursor"),
            ]),
            ..Default::default()
        },
        ..request
    }
    .into()
}

fn ages(ages: &[i64]) -> Value {
    Value::Array(ages.iter().map(|age| json!({ "age": age })).collect())
}

fn token() -> Value {
    json!({ "$match": "nonemptyString" })
}

/// A page of `rows` with `hasNext`/`hasPrev`, its cursors any non-empty
/// token (or null for an empty page).
fn rows(rows: Value, has_next: bool, has_prev: bool) -> Value {
    let empty = rows.as_array().is_some_and(Vec::is_empty);
    let cursor = if empty { Value::Null } else { token() };
    json!({ "_cursor": {
        "User": rows,
        "_pageInfo": { "hasNext": has_next, "hasPrev": has_prev, "startCursor": cursor, "endCursor": cursor },
    } })
}

fn run(actions: Vec<Actions>) {
    TestCase::from_actions(actions).execute();
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.token.keys")]
#[mantra_macros::req_test("go.shinzo.cursor.token.encoding")]
#[ignore = "#2481: Rust tokens carry an empty `o`, Go Shinzo tokens do not"]
fn shinzo_cursor_tokens_carry_their_rows_order_values() {
    let mut actions = aged(&[]);
    actions.extend([
        doc(json!({ "name": "Alice", "age": 25 }), None),
        doc(json!({ "name": "Bob", "age": 30 }), None),
        doc(json!({ "name": "Carol", "age": 35 }), None),
        page(
            "first: 2, order: {age: ASC}",
            None,
            "name",
            json!({ "_cursor": {
                "User": [{ "name": "Alice" }, { "name": "Bob" }],
                "_pageInfo": {
                    "hasNext": true,
                    "hasPrev": false,
                    "startCursor": ALICE_25,
                    "endCursor": BOB_30,
                },
            } }),
        )
        .into(),
    ]);
    run(actions);
}

/// `{"d":"bae-6064fa74-3ca9-5fc5-b08d-2bfcd839488d","k":{"age":25}}`, unpadded base64url, as Go Shinzo issues it.
const ALICE_25: &str =
    "eyJkIjoiYmFlLTYwNjRmYTc0LTNjYTktNWZjNS1iMDhkLTJiZmNkODM5NDg4ZCIsImsiOnsiYWdlIjoyNX19";
/// `{"d":"bae-b99540cd-c9aa-5b6a-9080-a97f918c1186","k":{"age":30}}`, unpadded base64url, as Go Shinzo issues it.
const BOB_30: &str =
    "eyJkIjoiYmFlLWI5OTU0MGNkLWM5YWEtNWI2YS05MDgwLWE5N2Y5MThjMTE4NiIsImsiOnsiYWdlIjozMH19";

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.token.invalid")]
fn shinzo_a_malformed_cursor_fails_the_request() {
    // `not json` and `{"k":{"age":25}}` as unpadded base64url, so the decoder
    // reaches the JSON and DocID checks.
    let mut actions = aged(&[25, 30]);
    for token in [
        "",
        "not-valid-base64!!!",
        "bm90IGpzb24",
        "eyJrIjp7ImFnZSI6MjV9fQ",
    ] {
        for arg in ["after", "before"] {
            let size = if arg == "after" { "first" } else { "last" };
            actions.push(
                Request {
                    request: format!(
                        "query {{ _cursor {{ User({size}: 1, {arg}: \"{token}\", order: {{age: ASC}}) {{ age }} _pageInfo {{ hasNext }} }} }}"
                    )
                    .into(),
                    expected_error: Some("invalid cursor".into()),
                    ..Default::default()
                }
                .into(),
            );
        }
    }
    run(actions);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.forward.first")]
fn shinzo_first_returns_the_first_rows_and_their_page_info() {
    let order = "order: {age: ASC}";
    let mut actions = vec![
        collection(USERS),
        page(
            &format!("first: 5, {order}"),
            None,
            "age",
            rows(json!([]), false, false),
        )
        .into(),
    ];
    actions.extend(aged(&[25, 30, 35, 40, 45]).into_iter().skip(1));
    for (first, want, has_next) in [
        (3, vec![25, 30, 35], true),
        (1, vec![25], true),
        (5, vec![25, 30, 35, 40, 45], false),
        (100, vec![25, 30, 35, 40, 45], false),
        (0, vec![], true),
    ] {
        actions.push(
            page(
                &format!("first: {first}, {order}"),
                None,
                "age",
                rows(ages(&want), has_next, false),
            )
            .into(),
        );
    }
    actions.push(
        page(
            &format!("first: 2, after: null, {order}"),
            None,
            "age",
            rows(ages(&[25, 30]), true, false),
        )
        .into(),
    );
    run(actions);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.forward.after")]
fn shinzo_after_resumes_page_by_page() {
    let order = "order: {age: ASC}";
    let mut actions = aged(&[10, 20, 30, 40, 50, 60, 70]);
    actions.extend([
        captured(
            page(
                &format!("first: 3, {order}"),
                None,
                "age",
                rows(ages(&[10, 20, 30]), true, false),
            ),
            "p1",
        ),
        captured(
            page(
                &format!("first: 3, {order}"),
                Some(("after", "p1_end")),
                "age",
                rows(ages(&[40, 50, 60]), true, true),
            ),
            "p2",
        ),
        captured(
            page(
                &format!("first: 3, {order}"),
                Some(("after", "p2_end")),
                "age",
                rows(ages(&[70]), false, true),
            ),
            "p3",
        ),
        page(
            &format!("first: 3, {order}"),
            Some(("after", "p3_end")),
            "age",
            rows(json!([]), false, true),
        )
        .into(),
        // Page sizes change between requests; a full last page has no next.
        page(
            &format!("first: 4, {order}"),
            Some(("after", "p1_end")),
            "age",
            rows(ages(&[40, 50, 60, 70]), false, true),
        )
        .into(),
        page(
            &format!("first: 1, {order}"),
            Some(("after", "p2_end")),
            "age",
            rows(ages(&[70]), false, true),
        )
        .into(),
    ]);
    run(actions);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.forward.filter")]

fn shinzo_a_filter_applies_before_paging() {
    let mut actions = aged(&[15, 25, 35, 45, 55, 65, 75]);
    let geq = "first: 2, filter: {age: {_geq: 40}}, order: {age: ASC}";
    actions.extend([
        captured(
            page(geq, None, "age", rows(ages(&[45, 55]), true, false)),
            "geq",
        ),
        page(
            geq,
            Some(("after", "geq_end")),
            "age",
            rows(ages(&[65, 75]), false, true),
        )
        .into(),
    ]);
    let within = "first: 2, filter: {age: {_in: [55, 25, 45, 35]}}, order: {age: ASC}";
    actions.extend([
        captured(
            page(within, None, "age", rows(ages(&[25, 35]), true, false)),
            "in",
        ),
        page(
            within,
            Some(("after", "in_end")),
            "age",
            rows(ages(&[45, 55]), false, true),
        )
        .into(),
    ]);
    run(actions);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.backward.last")]
fn shinzo_last_returns_the_final_rows() {
    let mut actions = aged(&[25, 30, 35, 40, 45]);
    actions.push(
        page(
            "last: 2, order: {age: ASC}",
            None,
            "age",
            rows(ages(&[40, 45]), false, true),
        )
        .into(),
    );
    run(actions);
    let mut few = aged(&[25, 30]);
    few.push(
        page(
            "last: 5, order: {age: ASC}",
            None,
            "age",
            rows(ages(&[25, 30]), false, false),
        )
        .into(),
    );
    run(few);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.backward.before")]
fn shinzo_before_returns_the_rows_just_before_its_row() {
    let order = "order: {age: ASC}";
    let mut actions = aged(&[25, 30, 35, 40, 45]);
    actions.extend([
        // A forward page ending at 40, then one ending at 35, then the first row.
        captured(
            page(
                &format!("first: 4, {order}"),
                None,
                "age",
                rows(ages(&[25, 30, 35, 40]), true, false),
            ),
            "to40",
        ),
        captured(
            page(
                &format!("first: 3, {order}"),
                None,
                "age",
                rows(ages(&[25, 30, 35]), true, false),
            ),
            "to35",
        ),
        page(
            &format!("last: 2, {order}"),
            Some(("before", "to40_end")),
            "age",
            rows(ages(&[30, 35]), true, true),
        )
        .into(),
        page(
            &format!("last: 10, {order}"),
            Some(("before", "to35_end")),
            "age",
            rows(ages(&[25, 30]), true, false),
        )
        .into(),
        page(
            &format!("last: 2, {order}"),
            Some(("before", "to40_start")),
            "age",
            rows(json!([]), true, false),
        )
        .into(),
    ]);
    run(actions);

    let mut walk = aged(&[10, 20, 30, 40, 50, 60]);
    walk.extend([
        captured(
            page(
                &format!("last: 2, {order}"),
                None,
                "age",
                rows(ages(&[50, 60]), false, true),
            ),
            "b1",
        ),
        captured(
            page(
                &format!("last: 2, {order}"),
                Some(("before", "b1_start")),
                "age",
                rows(ages(&[30, 40]), true, true),
            ),
            "b2",
        ),
        page(
            &format!("last: 2, {order}"),
            Some(("before", "b2_start")),
            "age",
            rows(ages(&[10, 20]), true, false),
        )
        .into(),
    ]);
    run(walk);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.backward.docid_only_token")]
fn shinzo_a_token_without_keys_still_pages_backward() {
    let mut actions = aged(&[10, 20, 30, 40, 50]);
    actions.push(
        Request {
            request: format!(
                "query {{ _cursor {{ User(last: 2, before: \"{U50_DOC_ID_ONLY}\", order: {{age: ASC}}) {{ age }} _pageInfo {{ hasNext hasPrev startCursor endCursor }} }} }}"
            )
            .into(),
            expected_data: rows(ages(&[30, 40]), true, true),
            ..Default::default()
        }
        .into(),
    );
    run(actions);
}

/// `{"d":"bae-4f2f018f-50b2-511e-aba4-af2d33de3051"}` (u50, no keys), unpadded base64url.
const U50_DOC_ID_ONLY: &str = "eyJkIjoiYmFlLTRmMmYwMThmLTUwYjItNTExZS1hYmE0LWFmMmQzM2RlMzA1MSJ9";

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.deleted_docs")]
#[ignore = "#2481: before a deleted row's cursor reports hasNext true"]
fn shinzo_deleted_documents_leave_pages_but_keep_positions() {
    let order = "order: {age: ASC}";
    let delete = |name: &str| -> Actions {
        DeleteDoc {
            collection: "User".into(),
            doc_id: Text::reference(name),
            ..Default::default()
        }
        .into()
    };
    let mut actions = aged(&[25, 30, 35]);
    actions.extend([
        captured(
            page(
                &format!("first: 2, {order}"),
                None,
                "age",
                rows(ages(&[25, 30]), true, false),
            ),
            "bob",
        ),
        captured(
            page(
                &format!("first: 1, {order}"),
                None,
                "age",
                rows(ages(&[25]), true, false),
            ),
            "alice",
        ),
        delete("u30"),
        page(
            &format!("first: 2, {order}"),
            Some(("after", "bob_end")),
            "age",
            rows(ages(&[35]), false, true),
        )
        .into(),
        delete("u35"),
        page(
            &format!("first: 10, {order}"),
            Some(("after", "alice_end")),
            "age",
            rows(json!([]), false, true),
        )
        .into(),
    ]);
    run(actions);

    let mut five = aged(&[25, 30, 35, 40, 45]);
    five.extend([
        captured(
            page(
                &format!("first: 5, {order}"),
                None,
                "age",
                rows(ages(&[25, 30, 35, 40, 45]), false, false),
            ),
            "eve",
        ),
        delete("u35"),
        page(
            &format!("first: 5, {order}"),
            None,
            "age",
            rows(ages(&[25, 30, 40, 45]), false, false),
        )
        .into(),
        delete("u45"),
        page(
            &format!("last: 2, {order}"),
            Some(("before", "eve_end")),
            "age",
            rows(ages(&[30, 40]), false, true),
        )
        .into(),
    ]);
    run(five);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.field_kinds")]
fn shinzo_string_float_and_datetime_order_fields_page_in_order() {
    let kinds: [(&str, &str, Vec<Value>); 3] = [
        (
            "name: String @index",
            "name",
            ["Islam", "Chris", "Addo", "Fred", "Andy"]
                .map(Value::from)
                .to_vec(),
        ),
        (
            "score: Float @index",
            "score",
            [4.2, 1.5, 5.8, 2.7, 3.9].map(Value::from).to_vec(),
        ),
        (
            "birthday: DateTime @index",
            "birthday",
            [
                "2003-09-25",
                "2000-01-15",
                "2004-12-01",
                "2001-03-20",
                "2002-06-10",
            ]
            .map(|day| Value::from(format!("{day}T00:00:00-00:00")))
            .to_vec(),
        ),
    ];
    for (sdl, field, values) in kinds {
        let mut sorted: Vec<Value> = values.clone();
        sorted.sort_by_key(Value::to_string);
        if field == "score" {
            sorted.sort_by(|a, b| a.as_f64().unwrap().total_cmp(&b.as_f64().unwrap()));
        }
        let shown = |value: &Value| -> Value {
            match field {
                "birthday" => Value::from(value.as_str().unwrap().replace("-00:00", "Z")),
                _ => value.clone(),
            }
        };
        let rows_of = |slice: &[Value]| {
            Value::Array(slice.iter().map(|v| json!({ field: shown(v) })).collect())
        };
        let order = format!("order: {{{field}: ASC}}");
        let mut actions = vec![collection(&format!("type User {{ {sdl} }}"))];
        actions.extend(
            values
                .iter()
                .map(|value| doc(json!({ field: value }), None)),
        );
        actions.extend([
            captured(
                page(
                    &format!("first: 2, {order}"),
                    None,
                    field,
                    rows(rows_of(&sorted[..2]), true, false),
                ),
                "k1",
            ),
            captured(
                page(
                    &format!("first: 2, {order}"),
                    Some(("after", "k1_end")),
                    field,
                    rows(rows_of(&sorted[2..4]), true, true),
                ),
                "k2",
            ),
            page(
                &format!("first: 2, {order}"),
                Some(("after", "k2_end")),
                field,
                rows(rows_of(&sorted[4..]), false, true),
            )
            .into(),
            page(
                &format!("last: 2, {order}"),
                None,
                field,
                rows(rows_of(&sorted[3..]), false, true),
            )
            .into(),
        ]);
        run(actions);
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.index.required")]
fn shinzo_an_order_without_a_matching_index_is_refused() {
    let refused = |sdl: &str, order: &str| -> Vec<Actions> {
        vec![
            collection(sdl),
            Request {
                request: format!("query {{ _cursor {{ User(first: 2, order: {order}) {{ name }} _pageInfo {{ hasNext }} }} }}").into(),
                expected_error: Some("no supporting index for cursor order field".into()),
                ..Default::default()
            }
            .into(),
        ]
    };
    run(refused("type User { name: String age: Int }", "{age: ASC}"));
    let composite = "type User @index(includes: [{field: \"name\"}, {field: \"age\"}]) { name: String age: Int email: String }";
    run(refused(composite, "{email: ASC}"));
    run(refused(composite, "{name: ASC}"));
    run(refused(
        "type User @index(includes: [{field: \"name\", direction: DESC}, {field: \"age\"}]) { name: String age: Int }",
        "[{name: ASC}, {age: ASC}]",
    ));
    // `_docID` order, or none, needs no index.
    let mut free = vec![
        collection("type User { name: String age: Int }"),
        doc(json!({ "name": "A", "age": 1 }), None),
    ];
    for args in ["first: 1, order: {_docID: ASC}", "first: 1"] {
        free.push(
            page(
                args,
                None,
                "name",
                rows(json!([{ "name": "A" }]), false, false),
            )
            .into(),
        );
    }
    run(free);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.index.direction")]
fn shinzo_an_index_serves_both_directions() {
    for (sdl, order, want) in [
        (USERS, "order: {age: DESC}", [[45, 40], [35, 30]]),
        (
            "type User { name: String age: Int @index(direction: DESC) }",
            "order: {age: ASC}",
            [[25, 30], [35, 40]],
        ),
    ] {
        let last = if order.contains("DESC") { 25 } else { 45 };
        let mut actions = vec![collection(sdl)];
        actions.extend(
            [25, 30, 35, 40, 45]
                .map(|age| doc(json!({ "name": format!("u{age}"), "age": age }), None)),
        );
        actions.extend([
            captured(
                page(
                    &format!("first: 2, {order}"),
                    None,
                    "age",
                    rows(ages(&want[0]), true, false),
                ),
                "d1",
            ),
            captured(
                page(
                    &format!("first: 2, {order}"),
                    Some(("after", "d1_end")),
                    "age",
                    rows(ages(&want[1]), true, true),
                ),
                "d2",
            ),
            page(
                &format!("first: 2, {order}"),
                Some(("after", "d2_end")),
                "age",
                rows(ages(&[last]), false, true),
            )
            .into(),
        ]);
        run(actions);
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.composite")]
fn shinzo_a_composite_order_resumes_within_a_shared_name() {
    let sdl = "type User @index(includes: [{field: \"name\"}, {field: \"age\"}]) { name: String age: Int }";
    let order = "order: [{name: ASC}, {age: ASC}]";
    let pair = |rows: &[(&str, i64)]| {
        Value::Array(
            rows.iter()
                .map(|(n, a)| json!({ "name": n, "age": a }))
                .collect(),
        )
    };
    let mut actions = vec![collection(sdl)];
    actions.extend(
        [
            ("Chris", 45),
            ("Addo", 35),
            ("Andy", 30),
            ("Addo", 25),
            ("Chris", 40),
        ]
        .map(|(name, age)| doc(json!({ "name": name, "age": age }), None)),
    );
    actions.extend([
        page(
            &format!("first: 3, {order}"),
            None,
            "name age",
            rows(
                pair(&[("Addo", 25), ("Addo", 35), ("Andy", 30)]),
                true,
                false,
            ),
        )
        .into(),
        captured(
            page(
                &format!("first: 2, {order}"),
                None,
                "name age",
                rows(pair(&[("Addo", 25), ("Addo", 35)]), true, false),
            ),
            "c1",
        ),
        captured(
            page(
                &format!("first: 2, {order}"),
                Some(("after", "c1_end")),
                "name age",
                rows(pair(&[("Andy", 30), ("Chris", 40)]), true, true),
            ),
            "c2",
        ),
        page(
            &format!("first: 2, {order}"),
            Some(("after", "c2_end")),
            "name age",
            rows(pair(&[("Chris", 45)]), false, true),
        )
        .into(),
    ]);
    run(actions);
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.shape")]
fn shinzo_aliases_and_group_by_inside_a_cursor() {
    let mut actions = aged(&[]);
    actions.extend([
        doc(json!({ "name": "Alice", "age": 25 }), None),
        doc(json!({ "name": "Bob", "age": 30 }), None),
        doc(json!({ "name": "Carol", "age": 35 }), None),
        Request {
            request: "query { _cursor { users: User(first: 2, order: {age: ASC}) { name age } _pageInfo { hasNext hasPrev } } }".into(),
            expected_data: json!({ "_cursor": {
                "users": [{ "name": "Alice", "age": 25 }, { "name": "Bob", "age": 30 }],
                "_pageInfo": { "hasNext": true, "hasPrev": false },
            } }),
            ..Default::default()
        }
        .into(),
    ]);
    run(actions);

    let mut grouped = vec![collection(USERS)];
    grouped.extend(
        [("a", 25), ("b", 25), ("c", 35), ("d", 45)]
            .map(|(name, age)| doc(json!({ "name": name, "age": age }), None)),
    );
    grouped.push(
        Request {
            request: "query { _cursor { User(first: 10, groupBy: [age]) { age } _pageInfo { hasNext hasPrev } } }".into(),
            expected_data: json!({ "_cursor": {
                "User": { "$match": "unordered", "items": [{ "age": 25 }, { "age": 35 }, { "age": 45 }] },
                "_pageInfo": { "hasNext": false, "hasPrev": false },
            } }),
            ..Default::default()
        }
        .into(),
    );
    run(grouped);
}

/// An execute explain whose scan reports `metrics`.
fn scanned(request: &str, metrics: Value) -> Actions {
    Request {
        request: format!("query @explain(type: execute) {{ _cursor {{ {request} _pageInfo {{ hasNext hasPrev }} }} }}").into(),
        expected_data: json!({ "$match": "explainScan", "metrics": metrics }),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.index.scan")]
fn shinzo_a_page_reads_only_the_rows_it_needs_from_the_index() {
    let mut actions = aged(&[25, 30, 35, 40, 45]);
    actions.extend([
        scanned(
            "User(first: 3, order: {age: ASC}) { age }",
            json!({ "indexFetches": 4 }),
        ),
        scanned("User(first: 3) { age }", json!({ "indexFetches": 0 })),
    ]);
    run(actions);

    for (sdl, field, values) in [
        (
            "name: String @index",
            "name",
            json!(["a", "b", "c", "d", "e"]),
        ),
        (
            "score: Float @index",
            "score",
            json!([1.5, 2.5, 3.5, 4.5, 5.5]),
        ),
        (
            "day: DateTime @index",
            "day",
            json!([
                "2000-01-01T00:00:00Z",
                "2001-01-01T00:00:00Z",
                "2002-01-01T00:00:00Z",
                "2003-01-01T00:00:00Z",
                "2004-01-01T00:00:00Z"
            ]),
        ),
    ] {
        let mut kind = vec![collection(&format!("type User {{ {sdl} }}"))];
        kind.extend(
            values
                .as_array()
                .unwrap()
                .iter()
                .map(|value| doc(json!({ field: value }), None)),
        );
        kind.push(scanned(
            &format!("User(first: 3, order: {{{field}: ASC}}) {{ {field} }}"),
            json!({ "indexFetches": 4 }),
        ));
        run(kind);
    }

    let mut composite = vec![collection("type User @index(includes: [{field: \"name\"}, {field: \"age\"}]) { name: String age: Int }")];
    composite.extend(
        [("a", 1), ("a", 2), ("b", 1), ("c", 1), ("d", 1)]
            .map(|(name, age)| doc(json!({ "name": name, "age": age }), None)),
    );
    composite.push(scanned(
        "User(first: 3, order: [{name: ASC}, {age: ASC}]) { name }",
        json!({ "indexFetches": 4 }),
    ));
    run(composite);

    let mut descending = vec![collection(USERS)];
    descending.extend(
        [45, 40, 35, 30, 25].map(|age| doc(json!({ "name": format!("u{age}"), "age": age }), None)),
    );
    descending.extend([
        scanned(
            "User(first: 5, order: {age: ASC}) { age }",
            json!({ "indexFetches": 5 }),
        ),
        page(
            "first: 5, order: {age: ASC}",
            None,
            "age",
            rows(ages(&[25, 30, 35, 40, 45]), false, false),
        )
        .into(),
    ]);
    run(descending);

    let mut three = aged(&[25, 30, 35]);
    three.push(scanned(
        "User(last: 2, order: {age: ASC}) { age }",
        json!({ "indexFetches": 3 }),
    ));
    run(three);

    let mut before = aged(&[10, 20, 30, 40, 50]);
    before.extend([
        captured(page("first: 5, order: {age: ASC}", None, "age", rows(ages(&[10, 20, 30, 40, 50]), false, false)), "all"),
        Request {
            request: Text::concat([
                Text::from("query @explain(type: execute) { _cursor { User(last: 2, order: {age: ASC}, before: \""),
                Text::reference("all_end"),
                Text::from("\") { age } _pageInfo { hasNext } } }"),
            ]),
            expected_data: json!({ "$match": "explainScan", "metrics": { "indexFetches": 3 } }),
            ..Default::default()
        }
        .into(),
    ]);
    run(before);
}

/// `{"$match":"object"}` over `fields`: those keys hold those values,
/// whatever else the object has.
fn object(fields: Value) -> Value {
    json!({ "$match": "object", "fields": fields })
}

/// An explain whose cursor node holds `cursor`.
fn cursor_node(cursor: Value) -> Value {
    json!({ "explain": object(json!({
        "operationNode": [{ "selectTopNode": object(json!({ "cursorNode": object(cursor) })) }],
    })) })
}

#[test]
#[mantra_macros::req_test("go.shinzo.cursor.explain")]
#[ignore = "#2481: cursorNode explains direction and pageSize, not Go's first/after/last/before/cursorValue"]
fn shinzo_a_cursor_query_explains_its_cursor_node() {
    let mut actions = aged(&[25, 30, 35]);
    let null = Value::Null;
    for (args, first, last) in [
        ("first: 5", json!(5), null.clone()),
        ("first: 3, after: null", json!(3), null.clone()),
        ("last: 2", null.clone(), json!(2)),
    ] {
        actions.push(
            Request {
                request: format!("query @explain {{ _cursor {{ User({args}, order: {{age: ASC}}) {{ name }} _pageInfo {{ hasNext }} }} }}").into(),
                expected_data: cursor_node(json!({
                    "first": first, "after": null, "last": last, "before": null, "cursorValue": null,
                })),
                ..Default::default()
            }
            .into(),
        );
    }
    actions.push(
        Request {
            request: "query @explain(type: execute) { _cursor { User(first: 2, order: {age: ASC}) { name } _pageInfo { hasNext } } }".into(),
            expected_data: json!({ "explain": object(json!({
                "executionSuccess": true,
                "sizeOfResult": 1,
                "planExecutions": 2,
                "operationNode": [{ "selectTopNode": object(json!({ "cursorNode": object(json!({ "iterations": 3 })) })) }],
            })) }),
            ..Default::default()
        }
        .into(),
    );
    run(actions);
}

/// An unsorted `_in` filter keeps the requested order with no cursor at
/// all, on an indexed and an unindexed field.
#[test]
#[mantra_macros::req_test("go.shinzo.cursor.forward.filter")]
fn shinzo_an_unsorted_in_filter_keeps_the_order_without_a_cursor() {
    for sdl in [
        USERS,
        "type User { name: String age: Int @index(direction: DESC) }",
        "type User { name: String age: Int }",
    ] {
        for (order, want) in [("ASC", [25, 35, 45, 55]), ("DESC", [55, 45, 35, 25])] {
            let mut actions = vec![collection(sdl)];
            actions.extend(
                [25, 35, 45, 55, 65]
                    .map(|age| doc(json!({ "name": format!("u{age}"), "age": age }), None)),
            );
            actions.push(
                Request {
                    request: format!("query {{ User(filter: {{age: {{_in: [55, 25, 45, 35]}}}}, order: {{age: {order}}}) {{ age }} }}")
                        .into(),
                    expected_data: json!({ "User": ages(&want) }),
                    ..Default::default()
                }
                .into(),
            );
            run(actions);
        }
    }
}
