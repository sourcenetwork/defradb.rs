//! Filter input validation at the parse-and-validate layer
//! (`defra.query.filter.input-validation`): what it checks, what it leaves
//! alone, and which collections it loads to do it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use query::query_parse::{parse_request_with_variables, validate_operation};
use query::{parse_sdl, CollectionProvider, Result, StaticCollectionProvider};
use rapidhash::{HashMapExt, RapidHashMap};
use schema::CollectionVersion;
use serde_json::{json, Value};

const SDL: &str = r#"
    type Author { name: String age: Int books: [Book] }
    type Book { title: String rating: Float author: Author }
"#;

/// A provider that counts the lookups validation makes.
struct Counting {
    inner: StaticCollectionProvider,
    gets: AtomicUsize,
    lists: AtomicUsize,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl CollectionProvider for Counting {
    async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        self.inner.get_collection(name).await
    }

    async fn list_collections(&self) -> Result<Vec<String>> {
        self.lists.fetch_add(1, Ordering::Relaxed);
        self.inner.list_collections().await
    }
}

fn provider(extra: usize) -> Counting {
    let mut sdl = SDL.to_string();
    for index in 0..extra {
        sdl.push_str(&format!("type Other{index} {{ x: Int }}\n"));
    }
    Counting {
        inner: StaticCollectionProvider::new(parse_sdl(&sdl).expect("valid SDL")),
        gets: AtomicUsize::new(0),
        lists: AtomicUsize::new(0),
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
fn a_relation_selection_without_a_filter_loads_no_other_collection() {
    let provider = provider(50);
    check_with(&provider, "{ Author { name books { title } } }", json!({})).unwrap();
    assert_eq!(provider.lists.load(Ordering::Relaxed), 0);
    assert!(provider.gets.load(Ordering::Relaxed) <= 2);
}

#[test]
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
fn a_filtered_relation_selection_is_still_checked() {
    let text = message(check(
        "{ Author { name books(filter: {nope: {_eq: 1}}) { title } } }",
        json!({}),
    ));
    assert!(text.contains("In field \"nope\": Unknown field."), "{text}");
}

#[test]
fn a_filter_under_an_unfiltered_relation_is_still_checked() {
    let text = message(check(
        "{ Author { books { author(filter: {nope: {_eq: 1}}) { name } } } }",
        json!({}),
    ));
    assert!(text.contains("In field \"nope\": Unknown field."), "{text}");
}

#[test]
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
fn a_valid_whole_list_variable_under_or_passes() {
    check(
        "query($c: [AuthorFilterArg!]) { Author(filter: {_or: $c}) { name } }",
        json!({ "c": [{ "name": { "_eq": "A" } }, { "age": { "_gt": 1 } }] }),
    )
    .unwrap();
}

#[test]
fn range_operators_on_a_string_field_pass() {
    check(
        r#"{ Author(filter: {name: {_gt: "B", _ge: "B", _geq: "B", _lt: "Z", _le: "Z", _leq: "Z"}}) { name } }"#,
        json!({}),
    )
    .unwrap();
}

#[test]
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
fn an_encrypted_query_without_an_encrypted_index_leaves_its_filter_unchecked() {
    check(
        "{ encrypted_Author(filter: {age: {_eq: 21}}) { docIDs } }",
        json!({}),
    )
    .unwrap();
}
