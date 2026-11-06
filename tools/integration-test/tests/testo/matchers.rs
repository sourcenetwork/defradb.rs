//! The scenario runner's matcher engine, one test per matcher kind.

use integration_test::m;
use integration_test::scenario::number::json_eq;
use integration_test::scenario::pointer::resolve;
use integration_test::scenario::{match_expected, validate, Bindings, Options};
use rapidhash::RapidHashMap;
use serde_json::{json, Value};

const CID: &str = "bafyreihqzhiz3iwro4jozp6kphq4sosg6ccoqcbiaf7rg5dmvea7aux55a";
const DOC_ID: &str = "bae-aad433b7-fe14-5a31-a5da-94735bedcd4f";

fn matches(expected: Value, actual: Value) -> bool {
    match_expected(
        &expected,
        &actual,
        Options::default(),
        &mut Bindings::default(),
    )
    .expect("valid expectation")
}

fn matches_with(expected: Value, actual: Value, unordered: &[&str], documents: &[&str]) -> bool {
    let unordered: Vec<String> = unordered.iter().map(|s| (*s).to_owned()).collect();
    let unordered_documents: Vec<String> = documents.iter().map(|s| (*s).to_owned()).collect();
    let options = Options {
        unordered: &unordered,
        unordered_documents: &unordered_documents,
    };
    match_expected(&expected, &actual, options, &mut Bindings::default()).expect("valid")
}

#[test]
fn literal_numbers_compare_as_exact_decimals() {
    assert!(matches(json!({"n": 1}), json!({"n": 1.0})));
    assert!(matches(json!({"n": 10}), json!({"n": 1e1})));
    assert!(!matches(json!({"n": 1}), json!({"n": "1"})));
    assert!(!matches(json!({"n": 1}), json!({"n": 2})));
    assert!(json_eq(
        &json!(9007199254740993_u64),
        &json!(9007199254740993_u64)
    ));
    assert!(!json_eq(
        &json!(9007199254740993_u64),
        &json!(9007199254740992_u64)
    ));
}

#[test]
fn literals_reject_extra_and_missing_keys() {
    assert!(!matches(json!({"a": 1}), json!({"a": 1, "b": 2})));
    assert!(!matches(json!({"a": 1, "b": 2}), json!({"a": 1})));
    assert!(!matches(json!({"a": null}), json!({})));
}

#[test]
fn unordered_option_sorts_only_the_named_array() {
    let expected = json!({"rows": [1, 2], "other": [1, 2]});
    let actual = json!({"rows": [2, 1], "other": [1, 2]});
    assert!(!matches(expected.clone(), actual.clone()));
    assert!(matches_with(expected, actual, &["/rows"], &[]));
    assert!(!matches_with(
        json!({"rows": [1, 2], "o": [1, 2]}),
        json!({"rows": [1, 2], "o": [2, 1]}),
        &["/rows"],
        &[]
    ));
}

#[test]
fn unordered_documents_sorts_document_arrays_below_the_root() {
    let expected = json!({"data": [{"a": 1, "l": [{"x": 1}, {"x": 2}]}, {"a": 2, "l": []}]});
    let actual = json!({"data": [{"a": 2, "l": []}, {"a": 1, "l": [{"x": 2}, {"x": 1}]}]});
    assert!(!matches(expected.clone(), actual.clone()));
    assert!(matches_with(expected, actual, &[], &["/data"]));
}

#[test]
fn object_matches_a_subset_and_allows_missing_optional() {
    let expected = m::object(json!({"a": 1, "b": m::optional(json!(2))}));
    assert!(matches(expected.clone(), json!({"a": 1, "c": 3})));
    assert!(matches(expected.clone(), json!({"a": 1, "b": 2})));
    assert!(!matches(expected, json!({"a": 1, "b": 3})));
}

#[test]
fn optional_inside_an_exact_object_still_rejects_extra_keys() {
    let expected = json!({"a": 1, "b": m::optional(json!(null))});
    assert!(matches(expected.clone(), json!({"a": 1})));
    assert!(matches(expected.clone(), json!({"a": 1, "b": null})));
    assert!(!matches(expected, json!({"a": 1, "c": 1})));
}

#[test]
fn unordered_matcher_backtracks_over_duplicates() {
    assert!(matches(m::unordered(json!([1, 1, 2])), json!([2, 1, 1])));
    assert!(!matches(m::unordered(json!([1, 1, 2])), json!([2, 2, 1])));
    assert!(!matches(m::unordered(json!([1])), json!([1, 1])));
}

#[test]
fn nonempty_string_needs_a_string_with_length() {
    assert!(matches(m::nonempty_string(), json!("x")));
    assert!(!matches(m::nonempty_string(), json!("")));
    assert!(!matches(m::nonempty_string(), json!(1)));
}

#[test]
fn nonempty_accepts_arrays_objects_and_strings() {
    assert!(matches(m::nonempty(), json!([1])));
    assert!(matches(m::nonempty(), json!({"a": 1})));
    assert!(matches(m::nonempty(), json!("a")));
    assert!(!matches(m::nonempty(), json!([])));
    assert!(!matches(m::nonempty(), json!(null)));
}

#[test]
fn same_binds_on_first_use_and_holds_across_steps() {
    let mut bindings = Bindings::default();
    let run = |expected: Value, actual: Value, bindings: &mut Bindings| {
        match_expected(&expected, &actual, Options::default(), bindings).expect("valid")
    };
    assert!(run(
        json!([m::same("a"), m::same("a")]),
        json!(["x", "x"]),
        &mut bindings
    ));
    assert!(run(m::same("a"), json!("x"), &mut bindings));
    assert!(!run(m::same("a"), json!("y"), &mut bindings));
}

#[test]
fn failed_match_does_not_commit_bindings() {
    let mut bindings = Bindings::default();
    let failed = json!([m::same("a"), 1]);
    assert!(!match_expected(&failed, &json!(["x", 2]), Options::default(), &mut bindings).unwrap());
    assert!(match_expected(
        &m::same("a"),
        &json!("y"),
        Options::default(),
        &mut bindings
    )
    .unwrap());
}

#[test]
fn unique_rejects_a_value_seen_before() {
    let mut bindings = Bindings::default();
    let run = |actual: Value, bindings: &mut Bindings| {
        match_expected(&m::unique("u"), &actual, Options::default(), bindings).unwrap()
    };
    assert!(run(json!(1), &mut bindings));
    assert!(run(json!(2), &mut bindings));
    assert!(!run(json!(1), &mut bindings));
    assert!(!matches(
        json!([m::unique("v"), m::unique("v")]),
        json!([1, 1])
    ));
}

#[test]
fn unordered_choice_backtracks_when_a_later_binding_conflicts() {
    let expected =
        m::unordered(json!([{"id": m::same("k"), "n": 1}, {"id": m::same("k"), "n": 2}]));
    let actual = json!([{"id": "a", "n": 2}, {"id": "a", "n": 1}]);
    assert!(matches(expected.clone(), actual));
    assert!(!matches(
        expected,
        json!([{"id": "a", "n": 2}, {"id": "b", "n": 1}])
    ));
}

#[test]
fn all_requires_every_clause() {
    assert!(matches(
        m::all(json!([m::nonempty_string(), m::substring("ab")])),
        json!("xaby")
    ));
    assert!(!matches(
        m::all(json!([m::nonempty_string(), m::substring("zz")])),
        json!("xaby")
    ));
}

#[test]
fn any_of_takes_the_first_alternative_that_matches() {
    let expected = m::any_of(json!([1, "a", m::nonempty()]));
    assert!(matches(expected.clone(), json!("a")));
    assert!(matches(expected.clone(), json!([0])));
    assert!(!matches(expected, json!(2)));
}

#[test]
fn cid_accepts_a_decodable_cid() {
    assert!(matches(m::cid(), json!(CID)));
    assert!(!matches(m::cid(), json!("not a cid")));
    assert!(!matches(m::cid(), json!(1)));
}

#[test]
fn doc_id_accepts_a_version_one_id() {
    assert!(matches(m::doc_id(), json!(DOC_ID)));
    assert!(!matches(m::doc_id(), json!("bae-nope")));
    assert!(!matches(m::doc_id(), json!(CID)));
}

#[test]
fn contains_allows_extra_elements() {
    assert!(matches(
        m::contains(json!([1, {"a": 1.0}])),
        json!([3, 1, {"a": 1}, 2])
    ));
    assert!(!matches(m::contains(json!([4])), json!([3, 1])));
    assert!(validate(&m::contains(json!([m::cid()]))).is_err());
}

#[test]
fn length_counts_array_object_and_string_bytes() {
    assert!(matches(m::length(2), json!([1, 2])));
    assert!(matches(m::length(1), json!({"a": 1})));
    assert!(matches(m::length(2), json!("é")));
    assert!(!matches(m::length(2), json!(5)));
}

#[test]
fn first_where_checks_only_the_first_matching_row() {
    let expected = m::first_where("name", "a", json!({"name": "a", "n": 1}));
    assert!(matches(
        expected.clone(),
        json!([{"name": "b", "n": 0}, {"name": "a", "n": 1}])
    ));
    assert!(!matches(
        expected.clone(),
        json!([{"name": "a", "n": 2}, {"name": "a", "n": 1}])
    ));
    assert!(!matches(expected, json!([{"name": "a", "n": 1}, 3])));
}

#[test]
fn substring_needs_a_containing_string() {
    assert!(matches(m::substring("bc"), json!("abcd")));
    assert!(!matches(m::substring("x"), json!("abcd")));
    assert!(!matches(m::substring("x"), json!(1)));
}

#[test]
fn similarity_score_recomputes_each_metric() {
    let source = json!([1.0, 0.0]);
    let vector = json!([0.0, 1.0]);
    assert!(matches(
        m::similarity_score("DOT", source.clone(), vector.clone()),
        json!(0.0)
    ));
    assert!(matches(
        m::similarity_score("EUCLIDEAN", source.clone(), vector.clone()),
        json!(-2.0)
    ));
    assert!(matches(
        m::similarity_score("COSINE", source.clone(), source.clone()),
        json!(1.0)
    ));
    assert!(matches(
        m::similarity_score("COSINE", json!([0.0, 0.0]), vector),
        json!(0.0)
    ));
    assert!(!matches(
        m::similarity_score("DOT", source.clone(), source),
        json!(0.5)
    ));
}

#[test]
fn current_timestamp_is_within_two_minutes_of_now() {
    let now = chrono::Utc::now();
    let near = (now - chrono::Duration::seconds(60)).to_rfc3339();
    let far = (now - chrono::Duration::seconds(600)).to_rfc3339();
    assert!(matches(m::current_timestamp(), json!(near)));
    assert!(!matches(m::current_timestamp(), json!(far)));
    assert!(!matches(m::current_timestamp(), json!("yesterday")));
}

#[test]
fn not_plain_cbor_string_differs_from_the_text_encoding() {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let plain = engine.encode([&[0x63u8][..], b"abc"].concat());
    let other = engine.encode(b"opaque");
    assert!(!matches(
        m::not_plain_cbor_string(json!("abc")),
        json!(plain)
    ));
    assert!(matches(
        m::not_plain_cbor_string(json!("abc")),
        json!(other)
    ));
    assert!(!matches(
        m::not_plain_cbor_string(json!("abc")),
        json!("!!!")
    ));
}

#[test]
fn introspection_types_require_fields_on_objects() {
    let good = json!([{"kind": "OBJECT", "fields": []}, {"kind": "SCALAR"}]);
    let bad = json!([{"kind": "OBJECT", "fields": null}]);
    assert!(matches(m::introspection_types(), good));
    assert!(!matches(m::introspection_types(), bad));
    assert!(!matches(
        m::introspection_types(),
        json!([{"kind": "INPUT_OBJECT"}])
    ));
}

#[test]
fn sorted_strings_are_non_decreasing_by_bytes() {
    assert!(matches(
        m::sorted_strings("n"),
        json!([{"n": "a"}, {"n": "a"}, {"n": "b"}])
    ));
    assert!(!matches(
        m::sorted_strings("n"),
        json!([{"n": "b"}, {"n": "a"}])
    ));
    assert!(!matches(m::sorted_strings("n"), json!([{"n": 1}])));
}

fn plan(scan: Value) -> Value {
    json!({"explain": {
        "executionSuccess": true,
        "sizeOfResult": 3,
        "planExecutions": 1,
        "operationNode": [{"selectTopNode": {"selectNode": {"filterMatches": 2, "scanNode": scan}}}]
    }})
}

#[test]
fn explain_scan_reads_the_requested_counters() {
    let actual = plan(json!({"iterations": 4, "docFetches": 3}));
    let metrics = json!({"iterations": 4, "docFetches": 3, "sizeOfResult": 3, "filterMatches": 2});
    assert!(matches(m::explain_scan(metrics), actual.clone()));
    assert!(!matches(
        m::explain_scan(json!({"iterations": 5})),
        actual.clone()
    ));
    assert!(matches(
        m::explain_scan(json!({"indexFetches": 0})),
        actual.clone()
    ));
    assert!(!matches(
        m::explain_scan_at(json!({"iterations": 4}), json!(["root"])),
        actual
    ));
}

#[test]
fn explain_pattern_compares_the_plan_node_skeleton() {
    let actual = json!({"explain": {
        "executionSuccess": true,
        "operationNode": [{"selectTopNode": {"selectNode": {"scanNode": {"collectionName": "Users"}}}}]
    }});
    let pattern = json!({"explain": {"operationNode": [{"selectTopNode": {"selectNode": {"scanNode": {}}}}]}});
    assert!(matches(m::explain_pattern(pattern), actual.clone()));
    let wrong = json!({"explain": {"operationNode": [{"selectTopNode": {"selectNode": {}}}]}});
    assert!(!matches(m::explain_pattern(wrong), actual));
}

#[test]
fn explain_target_finds_the_named_node() {
    let actual = json!({"explain": {"operationNode": [{"selectTopNode": {"selectNode": {
        "scanNode": {"collectionName": "Users", "spans": [1]}
    }}}]}});
    let attributes = json!({"collectionName": "Users", "spans": [1]});
    assert!(matches(
        m::explain_target("scanNode", false, 0, attributes),
        actual.clone()
    ));
    assert!(!matches(
        m::explain_target("scanNode", false, 1, json!({})),
        actual.clone()
    ));
    assert!(!matches(
        m::explain_target("limitNode", false, 0, json!({})),
        actual
    ));
}

fn commit(cid: &str, links: &[&str], heads: &[&str]) -> Value {
    let ids = |items: &[&str]| -> Vec<Value> { items.iter().map(|c| json!({"cid": c})).collect() };
    json!({"cid": cid, "links": ids(links), "heads": ids(heads)})
}

#[test]
fn branchable_create_graph_validates_the_two_document_shape() {
    let rows = vec![
        commit("l1", &[], &[]),
        commit("l2", &[], &[]),
        commit("l3", &[], &[]),
        commit("l4", &[], &[]),
        commit("c1", &["l1", "l2"], &[]),
        commit("c2", &["l3", "l4"], &[]),
        commit("root", &["c1"], &[]),
        commit("head", &["c2"], &["root"]),
    ];
    let actual = json!({"_commits": rows});
    let expected = m::branchable_create_graph("g");
    assert!(matches(expected.clone(), actual));
    assert!(!matches(expected, json!({"_commits": []})));
}

#[test]
fn matcher_shapes_are_validated_before_matching() {
    for bad in [
        json!({"$match": "nope"}),
        json!({"$match": "object", "fields": {}}),
        json!({"$match": "same"}),
        json!({"$match": "length", "value": -1}),
        json!({"$match": "cid", "extra": 1}),
        json!({"$match": "all", "of": []}),
    ] {
        assert!(validate(&bad).is_err(), "{bad} should be invalid");
    }
    assert!(validate(&json!({"plain": [1, 2]})).is_ok_and(|has| !has));
    assert!(validate(&json!({"plain": [m::cid()]})).is_ok_and(|has| has));
}

#[test]
fn ref_and_concat_resolve_against_captures() {
    let mut captures: RapidHashMap<String, Value> = RapidHashMap::default();
    captures.insert("id".to_owned(), json!("abc"));
    let value = json!({"a": m::r("id"), "b": [m::concat(["x-", "y"]), m::concat([json!("p"), m::r("id")])]});
    let resolved = resolve(&value, &captures).expect("resolves");
    assert_eq!(resolved, json!({"a": "abc", "b": ["x-y", "pabc"]}));
    assert!(resolve(&m::r("missing"), &captures).is_err());
    assert!(resolve(&json!({"$concat": []}), &captures).is_err());
}
