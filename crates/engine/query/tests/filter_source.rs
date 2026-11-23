//! The parser records where each filter came from, so validation checks
//! what the request wrote against its input type and a variable's value
//! only as coercion does (`defra.query.filter.input-validation`).

use query::mapper::{FilterSource, PathStep, Requestable};
use query::query_parse::parse_request_with_variables;
use query::{Mutation, ParsedOperation};
use rapidhash::{HashMapExt, RapidHashMap};
use serde_json::{json, Value};

fn variables(value: Value) -> RapidHashMap<String, Value> {
    let mut map = RapidHashMap::new();
    if let Value::Object(fields) = value {
        map.extend(fields);
    }
    map
}

fn mutation(query: &str, vars: Value) -> Mutation {
    match parse_request_with_variables(query, Some(&variables(vars)), None).unwrap() {
        ParsedOperation::Mutation { mut mutations, .. } => mutations.remove(0),
        other => panic!("not a mutation: {other:?}"),
    }
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_truncate_filter_variable_keeps_its_source_and_the_doc_ids_fold_in_later() {
    let truncate = mutation(
        "mutation($f: UserFilterArg) { truncate_User(docID: [\"a\"], filter: $f) }",
        json!({ "f": { "name": { "_eq": "x" } } }),
    );
    let filter = truncate.filter.as_ref().unwrap();
    assert_eq!(filter.source(), &FilterSource::Variable("f".into()));
    assert!(!filter.conditions().contains_key("_docID"));

    let folded = truncate.truncate_filter().unwrap();
    let text = Value::Object(folded.conditions().clone()).to_string();
    assert!(text.contains("\"_docID\":{\"_in\":[\"a\"]}"), "{text}");
    assert!(text.contains("\"name\":{\"_eq\":\"x\"}"), "{text}");
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_truncate_without_doc_ids_keeps_its_filter() {
    let truncate = mutation(
        "mutation { truncate_User(filter: {name: {_eq: \"x\"}}) }",
        json!({}),
    );
    assert_eq!(truncate.truncate_filter(), truncate.filter);
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_nested_variable_is_recorded_at_its_position() {
    let parsed = parse_request_with_variables(
        "query($n: String) { User(filter: {name: {_eq: $n}}) { name } }",
        Some(&variables(json!({ "n": "x" }))),
        None,
    )
    .unwrap();
    let ParsedOperation::Query { selects, .. } = parsed else {
        panic!("not a query");
    };
    let FilterSource::LiteralWithVariables(positions) =
        selects[0].filter.as_ref().unwrap().source()
    else {
        panic!("not a literal with variables");
    };
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].name, "n");
    assert_eq!(
        positions[0].path,
        vec![PathStep::Key("name".into()), PathStep::Key("_eq".into())]
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_count_selector_variable_marks_its_filter() {
    let parsed = parse_request_with_variables(
        "query($t: AuthorBooksFilterArg) { Author { COUNT(books: $t) } }",
        Some(&variables(
            json!({ "t": { "filter": { "title": { "_eq": "x" } } } }),
        )),
        None,
    )
    .unwrap();
    let ParsedOperation::Query { selects, .. } = parsed else {
        panic!("not a query");
    };
    let aggregate = selects[0]
        .fields
        .iter()
        .find_map(|field| match field {
            Requestable::Aggregate(aggregate) => Some(aggregate),
            _ => None,
        })
        .unwrap();
    let filter = aggregate.targets[0].filter.as_ref().unwrap();
    assert_eq!(filter.source(), &FilterSource::SelectorVariable("t".into()));
}
