//! Filter input validation at the parse-and-validate layer
//! (`defra.query.filter.input-validation`): what it checks, what it leaves
//! alone, and which collections it loads to do it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use query::query_parse::{parse_request_with_variables, validate_operation};
use query::{parse_sdl, CollectionProvider, QueryError, Result, StaticCollectionProvider};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;
use serde_json::{json, Value};

const SDL: &str = r#"
    type Author { name: String age: Int tags: [String!] books: [Book] }
    type Book { title: String rating: Float author: Author }
"#;

/// A provider that counts the lookups validation makes, and can fail them
/// as a store read would.
struct Counting {
    inner: StaticCollectionProvider,
    gets: AtomicUsize,
    lists: AtomicUsize,
    failing_get: Option<&'static str>,
    failing_list: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CollectionProvider for Counting {
    async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        if self.failing_get == Some(name) {
            return Err(QueryError::internal("the store read failed"));
        }
        self.inner.get_collection(name).await
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        self.lists.fetch_add(1, Ordering::Relaxed);
        if self.failing_list {
            return Err(QueryError::internal("the store read failed"));
        }
        self.inner.list_collections().await
    }
}

fn provider(extra: usize) -> Counting {
    let mut sdl = SDL.to_string();
    for index in 0..extra {
        sdl.push_str(&format!("type Other{index} {{ x: Int }}\n"));
    }
    provider_for(&sdl)
}

fn provider_for(sdl: &str) -> Counting {
    Counting {
        inner: StaticCollectionProvider::new(parse_sdl(sdl).expect("valid SDL")),
        gets: AtomicUsize::new(0),
        lists: AtomicUsize::new(0),
        failing_get: None,
        failing_list: false,
    }
}

fn check_with(provider: &Counting, request: &str, variables: Value) -> Result<()> {
    let mut map = RapidHashMap::new();
    if let Value::Object(fields) = variables {
        map.extend(fields);
    }
    let op = parse_request_with_variables(request, Some(&map), None)?;
    workers::block_on(async { validate_operation(&op, provider, true, true).await })
        .expect("worker runs")
}

fn check(request: &str, variables: Value) -> Result<()> {
    check_with(&provider(0), request, variables)
}

fn message(result: Result<()>) -> String {
    result.expect_err("refused").to_string()
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_relation_selection_without_a_filter_loads_no_other_collection() {
    let provider = provider(50);
    check_with(&provider, "{ Author { name books { title } } }", json!({})).unwrap();
    assert_eq!(provider.lists.load(Ordering::Relaxed), 0);
    assert!(provider.gets.load(Ordering::Relaxed) <= 2);
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_relation_aggregate_without_a_filter_loads_no_other_collection() {
    let provider = provider(50);
    check_with(
        &provider,
        "{ Author { name _count(books: {}) } }",
        json!({}),
    )
    .unwrap();
    assert_eq!(provider.lists.load(Ordering::Relaxed), 0);
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_filtered_relation_selection_is_still_checked() {
    let text = message(check(
        "{ Author { name books(filter: {nope: {_eq: 1}}) { title } } }",
        json!({}),
    ));
    assert!(text.contains("In field \"nope\": Unknown field."), "{text}");
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_filter_under_an_unfiltered_relation_is_still_checked() {
    let text = message(check(
        "{ Author { books { author(filter: {nope: {_eq: 1}}) { name } } } }",
        json!({}),
    ));
    assert!(text.contains("In field \"nope\": Unknown field."), "{text}");
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_whole_list_variable_under_and_is_checked() {
    let text = message(check(
        "query($c: [AuthorFilterArg!]) { Author(filter: {_and: $c}) { name } }",
        json!({ "c": [{ "nope": { "_eq": 1 } }] }),
    ));
    assert_eq!(
        text,
        "Variable \"$c\" got invalid value [{\"nope\":{\"_eq\":1}}].\nIn element #1: In field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_valid_whole_list_variable_under_or_passes() {
    check(
        "query($c: [AuthorFilterArg!]) { Author(filter: {_or: $c}) { name } }",
        json!({ "c": [{ "name": { "_eq": "A" } }, { "age": { "_gt": 1 } }] }),
    )
    .unwrap();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn range_operators_on_a_string_field_pass() {
    check(
        r#"{ Author(filter: {name: {_gt: "B", _ge: "B", _geq: "B", _lt: "Z", _le: "Z", _leq: "Z"}}) { name } }"#,
        json!({}),
    )
    .unwrap();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_unknown_filter_under_commits_links_is_rejected() {
    assert_eq!(
        message(check(
            "{ _commits { cid links(filter: {nope: {_eq: 1}}) { cid } } }",
            json!({}),
        )),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_unknown_filter_under_commits_heads_is_rejected() {
    assert_eq!(
        message(check(
            "{ _commits { cid heads(filter: {nope: {_eq: 1}}) { cid } } }",
            json!({}),
        )),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_unknown_filter_under_version_links_is_rejected() {
    assert_eq!(
        message(check(
            "{ Author { name _version { cid links(filter: {nope: {_eq: 1}}) { cid } } } }",
            json!({}),
        )),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_unknown_filter_under_version_heads_is_rejected() {
    assert_eq!(
        message(check(
            "{ Author { name _version { cid heads(filter: {nope: {_eq: 1}}) { cid } } } }",
            json!({}),
        )),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_valid_filter_under_version_links_passes() {
    check(
        "{ Author { name _version { cid links(filter: {fieldName: {_eq: \"age\"}}) { cid } } } }",
        json!({}),
    )
    .unwrap();
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_unknown_filter_on_group_is_rejected() {
    assert_eq!(
        message(check(
            "{ Author(groupBy: [name]) { name GROUP(filter: {nope: {_eq: 1}}) { age } } }",
            json!({}),
        )),
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_valid_group_filter_passes() {
    check(
        "{ Author(groupBy: [name]) { name GROUP(filter: {age: {_gt: 1}}) { age } } }",
        json!({}),
    )
    .unwrap();
}

/// Go generates no `encrypted_<Collection>` field for a collection without
/// an encrypted index, so its filter is never checked: the request fails on
/// the missing field when it runs.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_encrypted_query_without_an_encrypted_index_leaves_its_filter_unchecked() {
    check(
        "{ encrypted_Author(filter: {age: {_eq: 21}}) { docIDs } }",
        json!({}),
    )
    .unwrap();
}

/// In a three-collection cycle `A.b -> B.c -> C.a`, `A.b` is a self
/// reference to B's place in the set: `b`'s filter is B's.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_filter_through_a_three_collection_cycle_is_checked_against_its_target() {
    let provider = provider_for(
        "type A { name: String b: B } type B { title: String c: C } type C { x: Int a: A }",
    );
    check_with(
        &provider,
        r#"{ A(filter: {b: {title: {_eq: "x"}}}) { name } }"#,
        json!({}),
    )
    .unwrap();
    let text = message(check_with(
        &provider,
        r#"{ A(filter: {b: {name: {_eq: "x"}}}) { name } }"#,
        json!({}),
    ));
    assert!(
        text.contains("In field \"b\": In field \"name\": Unknown field."),
        "{text}"
    );
}

/// Go decodes a variable's numbers into `int64` or `float64` and prints
/// them with `json.Marshal`: a whole float prints without its fraction, a
/// large or small one in Go's exponent form, and an integer past `int64`
/// as the float it became.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_variable_prints_its_numbers_as_go_does() {
    let text = message(check(
        "query($f: AuthorFilterArg) { Author(filter: $f) { name } }",
        json!({ "f": { "nope": { "_in": [1.0, 2.5, -0.5, 1e17, 1e21, 1.5e-7, 18446744073709551615u64, 7] } } }),
    ));
    assert_eq!(
        text,
        "Variable \"$f\" got invalid value {\"nope\":{\"_in\":[1,2.5,-0.5,100000000000000000,1e+21,1.5e-7,18446744073709552000,7]}}.\nIn field \"nope\": Unknown field."
    );
}

/// A known literal error stands even when the relation lookup a later
/// selection's filter needs fails.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_literal_error_is_not_replaced_by_a_failed_relation_lookup() {
    let mut provider = provider(0);
    provider.failing_list = true;
    let text = message(check_with(
        &provider,
        r#"{ Author(filter: {nope: {_eq: 1}}) { books(filter: {title: {_eq: "x"}}) { title } } }"#,
        json!({}),
    ));
    assert_eq!(
        text,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

/// Once a root has a literal error, a later root's collection is not
/// looked up, so a failed lookup cannot replace that error.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_literal_error_is_not_replaced_by_a_later_roots_failed_lookup() {
    let mut provider = provider(0);
    provider.failing_get = Some("Book");
    let text = message(check_with(
        &provider,
        r#"{ Author(filter: {nope: {_eq: 1}}) { name } Book(filter: {title: {_eq: "x"}}) { title } }"#,
        json!({}),
    ));
    assert_eq!(
        text,
        "Argument \"filter\" has invalid value {nope: {_eq: 1}}.\nIn field \"nope\": Unknown field."
    );
}

/// A root with no filter anywhere below it has nothing to check, so the
/// filter pass looks nothing up: only the collection check does.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_root_without_a_filter_is_looked_up_once() {
    let provider = provider(0);
    check_with(&provider, "{ Author { _docID } }", json!({})).unwrap();
    assert_eq!(provider.gets.load(Ordering::Relaxed), 1);
    assert_eq!(provider.lists.load(Ordering::Relaxed), 0);
}

/// Relation filters under several roots load the other collections once
/// for the request, not once per root.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn relation_filters_under_several_roots_load_the_collections_once() {
    let provider = provider(0);
    check_with(
        &provider,
        r#"{ a: Author { books(filter: {title: {_eq: "x"}}) { title } } b: Author { books(filter: {title: {_eq: "y"}}) { title } } }"#,
        json!({}),
    )
    .unwrap();
    assert_eq!(provider.lists.load(Ordering::Relaxed), 1);
}

/// With no `encrypted_<Collection>` field to check arguments on, a
/// non-object filter variable is left to the missing-field error too.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn an_encrypted_query_without_an_encrypted_index_leaves_a_non_object_variable_unchecked() {
    check(
        "query($f: AuthorEncryptedFilterArg) { encrypted_Author(filter: $f) { docIDs } }",
        json!({ "f": 7 }),
    )
    .unwrap();
}

/// A non-object variable where an operator block belongs fails with the
/// block's name, as graphql-go's coercion names it.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_non_object_variable_for_an_operator_block_names_the_block() {
    let cases = [
        ("{name: $v}", "StringOperatorBlock"),
        ("{age: $v}", "IntOperatorBlock"),
        ("{_docID: $v}", "IDOperatorBlock"),
        ("{tags: $v}", "NotNullStringListOperatorBlock"),
        ("{tags: {_any: $v}}", "NotNullStringOperatorBlock"),
    ];
    for (filter, block) in cases {
        let text = message(check(
            &format!("query($v: {block}) {{ Author(filter: {filter}) {{ name }} }}"),
            json!({ "v": 7 }),
        ));
        assert_eq!(
            text,
            format!(
                "Variable \"$v\" got invalid value 7.\nExpected \"{block}\", found not an object."
            ),
            "{filter}"
        );
    }
}

/// A scalar list's aggregate `filter` is the element's `FilterArg`.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_non_object_aggregate_filter_on_a_scalar_list_names_its_filter_arg() {
    let text = message(check(
        "query($f: NotNullStringFilterArg) { Author { COUNT(tags: {filter: $f}) } }",
        json!({ "f": 7 }),
    ));
    assert_eq!(
        text,
        "Variable \"$f\" got invalid value 7.\nExpected \"NotNullStringFilterArg\", found not an object."
    );
}

/// A subscription open check refuses only an invalid request: a failed
/// lookup during validation is not one, an unknown filter field is.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_failed_lookup_does_not_make_the_request_invalid() {
    let mut failing = provider(0);
    failing.failing_list = true;
    let lookup = check_with(
        &failing,
        r#"{ Author { books(filter: {title: {_eq: "x"}}) { title } } }"#,
        json!({}),
    )
    .expect_err("the lookup fails");
    assert!(!lookup.is_invalid_request(), "{lookup}");

    let unknown = check("{ Author(filter: {nope: {_eq: 1}}) { name } }", json!({}))
        .expect_err("an unknown field is refused");
    assert!(unknown.is_invalid_request(), "{unknown}");
}

/// A cursor argument written wrong is an invalid request, so a subscription
/// open check refuses it instead of keeping the stream.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_bad_cursor_argument_makes_the_request_invalid() {
    for request in [
        "{ _cursor { Author(first: -1) { name } } }",
        "{ _cursor { Author(first: 1, last: 1) { name } } }",
        "{ _cursor { Author(after: \"\") { name } } }",
    ] {
        let error = check(request, json!({})).expect_err("refused");
        assert!(error.is_invalid_request(), "{request}: {error}");
    }
}

/// Of two bad variables, the one reported is the first the request uses,
/// whether or not a directive left that mutation out.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_left_out_mutation_s_variable_keeps_its_place_in_the_request() {
    let request = "mutation($first: AuthorFilterArg, $second: AuthorFilterArg) {
        a: delete_Author(filter: $first) @skip(if: true) { _docID }
        b: delete_Author(filter: $second) { _docID }
    }";
    let text = message(check(
        request,
        json!({ "first": { "nope": { "_eq": 1 } }, "second": { "nada": { "_eq": 1 } } }),
    ));
    assert!(text.starts_with("Variable \"$first\""), "{text}");
}

/// `COUNT(GROUP: …)` filters the grouped collection itself.
#[test]
#[mantra_macros::req_test("defra.query.filter.input-validation")]
fn a_group_count_filter_is_checked_against_its_collection() {
    let text = message(check(
        "{ Author(groupBy: [name]) { name COUNT(GROUP: {filter: {nope: {_eq: 1}}}) } }",
        json!({}),
    ));
    assert_eq!(
        text,
        "Argument \"GROUP\" has invalid value {filter: {nope: {_eq: 1}}}.\nIn field \"filter\": In field \"nope\": Unknown field."
    );
    check(
        r#"{ Author(groupBy: [name]) { name COUNT(GROUP: {filter: {name: {_eq: "A"}}}) } }"#,
        json!({}),
    )
    .expect("a known field is accepted");
}
