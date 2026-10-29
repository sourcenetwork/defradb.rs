use super::request::{add, delete_with_filter, input, update_with_filter};
use super::{AddOptions, Filter};
use serde_json::json;
#[test]
fn test_json_to_graphql_null() {
    assert_eq!(input(&json!(null)).unwrap(), "null");
}

#[test]
fn test_json_to_graphql_bool() {
    assert_eq!(input(&json!(true)).unwrap(), "true");
    assert_eq!(input(&json!(false)).unwrap(), "false");
}

#[test]
fn test_json_to_graphql_number() {
    assert_eq!(input(&json!(42)).unwrap(), "42");
    assert_eq!(input(&json!(-17)).unwrap(), "-17");
    assert_eq!(input(&json!(3.15)).unwrap(), "3.15");
}

#[test]
fn test_json_to_graphql_simple_string() {
    assert_eq!(input(&json!("hello")).unwrap(), "\"hello\"");
    assert_eq!(input(&json!("")).unwrap(), "\"\"");
}

#[test]
fn test_json_to_graphql_string_with_quotes() {
    assert_eq!(
        input(&json!("Hello \"World\"")).unwrap(),
        "\"Hello \\\"World\\\"\""
    );
}

#[test]
fn test_json_to_graphql_string_with_backslashes() {
    assert_eq!(
        input(&json!("path\\to\\file")).unwrap(),
        "\"path\\\\to\\\\file\""
    );
}

#[test]
fn test_json_to_graphql_string_with_newlines() {
    assert_eq!(input(&json!("line1\nline2")).unwrap(), "\"line1\\nline2\"");
}

#[test]
fn test_json_to_graphql_string_with_carriage_return() {
    assert_eq!(input(&json!("line1\rline2")).unwrap(), "\"line1\\rline2\"");
}

#[test]
fn test_json_to_graphql_string_with_tabs() {
    assert_eq!(input(&json!("col1\tcol2")).unwrap(), "\"col1\\tcol2\"");
}

#[test]
fn test_json_to_graphql_string_with_backspace() {
    assert_eq!(input(&json!("a\u{0008}b")).unwrap(), "\"a\\bb\"");
}

#[test]
fn test_json_to_graphql_string_with_form_feed() {
    assert_eq!(input(&json!("a\u{000c}b")).unwrap(), "\"a\\fb\"");
}

#[test]
fn test_json_to_graphql_string_with_other_controls() {
    // JSON (and Go valueToGQL) use \uXXXX for controls without a short escape.
    assert_eq!(input(&json!("a\u{0000}b")).unwrap(), "\"a\\u0000b\"");
    assert_eq!(input(&json!("a\u{0007}b")).unwrap(), "\"a\\u0007b\"");
}

#[test]
fn test_json_to_graphql_string_with_mixed_escapes() {
    assert_eq!(
        input(&json!("line1\nline2\t\"quoted\"\r\\end")).unwrap(),
        "\"line1\\nline2\\t\\\"quoted\\\"\\r\\\\end\""
    );
}

#[test]
fn test_json_to_graphql_array_empty() {
    assert_eq!(input(&json!([])).unwrap(), "[]");
}

#[test]
fn test_json_to_graphql_array_simple() {
    assert_eq!(input(&json!([1, 2, 3])).unwrap(), "[1, 2, 3]");
}

#[test]
fn test_json_to_graphql_array_mixed() {
    assert_eq!(
        input(&json!(["hello", 42, true, null])).unwrap(),
        "[\"hello\", 42, true, null]"
    );
}

#[test]
fn test_json_to_graphql_array_nested() {
    assert_eq!(input(&json!([[1, 2], [3, 4]])).unwrap(), "[[1, 2], [3, 4]]");
}

#[test]
fn test_json_to_graphql_object_simple() {
    let result = input(&json!({"name": "Alice", "age": 30})).unwrap();
    assert!(
        result == "{name: \"Alice\", age: 30}" || result == "{age: 30, name: \"Alice\"}",
        "Unexpected result: {}",
        result
    );
}

#[test]
fn test_json_to_graphql_object_nested() {
    let result = input(&json!({"user": {"name": "Bob"}})).unwrap();
    assert_eq!(result, "{user: {name: \"Bob\"}}");
}

#[test]
fn test_json_to_graphql_object_with_array() {
    let result = input(&json!({"tags": ["a", "b"]})).unwrap();
    assert_eq!(result, "{tags: [\"a\", \"b\"]}");
}

#[test]
fn test_json_to_graphql_complex_nested() {
    let value = json!({
        "user": {
            "name": "Alice\nSmith",
            "tags": ["admin", "user"],
            "active": true
        }
    });
    let result = input(&value).unwrap();
    assert!(result.contains("name: \"Alice\\nSmith\""));
    assert!(result.contains("tags: [\"admin\", \"user\"]"));
    assert!(result.contains("active: true"));
}

#[test]
fn test_json_to_graphql_unicode() {
    assert_eq!(input(&json!("héllo 世界")).unwrap(), "\"héllo 世界\"");
}
// ---------------------------------------------------------------------------
// Object keys are the trust boundary
// ---------------------------------------------------------------------------

/// Keys are written into the mutation unquoted, so a key carrying GraphQL
/// punctuation would close the argument list and let a caller append
/// operations of its own. This is the check that it cannot.
#[test]
fn a_key_that_could_escape_the_document_is_refused() {
    let hostile = [
        json!({"a) { _docID } } mutation { delete_Other(filter: {": 1}),
        json!({"name\"": 1}),
        json!({"na me": 1}),
        json!({"": 1}),
        json!({"1name": 1}),
        json!({"name#comment": 1}),
        json!({"a$b": 1}),
    ];
    for value in hostile {
        assert!(input(&value).is_err(), "{value} should be refused");
    }
}

/// The refusal has to reach nested keys too, or a filter one level down is
/// still an open door.
#[test]
fn a_hostile_key_is_refused_at_any_depth() {
    assert!(input(&json!({"user": {"na)me": 1}})).is_err());
    assert!(input(&json!({"users": [{"na)me": 1}]})).is_err());
    assert!(input(&json!([[{"na)me": 1}]])).is_err());
}

/// Nothing legitimate is turned away: a key that is not a GraphQL Name cannot
/// address a schema field in the first place.
#[test]
fn real_field_names_still_encode() {
    for value in [
        json!({"name": "Alice"}),
        json!({"_docID": "bae-1"}),
        json!({"age_2": 1}),
        json!({"_": 1}),
        json!({"A1": 1}),
        json!({"user": {"address": {"city": "Berlin"}}}),
    ] {
        assert!(input(&value).is_ok(), "{value} should encode");
    }
}

/// String *values* are still free-form; only keys are constrained.
#[test]
fn hostile_text_in_a_value_is_escaped_not_refused() {
    let encoded = input(&json!({"name": "a) { _docID } } mutation {"})).unwrap();
    assert_eq!(
        encoded, r#"{name: "a) { _docID } } mutation {"}"#,
        "a value is a quoted StringValue, so it cannot escape"
    );
}

/// The filter reaches the document builder verbatim, so the refusal has to
/// hold at the builder every edge calls, not only at the
/// encoder underneath it.
#[test]
fn a_filtered_delete_refuses_a_hostile_filter_key() {
    let hostile = json!({"a) { _docID } } mutation { delete_Other(filter: {": 1});
    assert!(delete_with_filter("Users", &Filter::from(hostile)).is_err());

    let ordinary = json!({"name": {"_eq": "Alice"}});
    assert_eq!(
        delete_with_filter("Users", &Filter::from(ordinary)).unwrap(),
        r#"mutation { delete_Users(filter: {name: {_eq: "Alice"}}) { _docID } }"#
    );
}

/// The update carries two caller-supplied objects, so both the filter and the
/// updater have to be refused when they could escape the document.
#[test]
fn a_filtered_update_refuses_a_hostile_key_in_either_half() {
    let hostile = json!({"a) { _docID } } mutation { delete_Other(filter: {": 1});
    let ordinary = json!({"name": {"_eq": "Alice"}});
    let patch = json!({"age": 31});

    assert!(
        update_with_filter("Users", &Filter::from(hostile.clone()), &patch.to_string()).is_err()
    );
    assert!(update_with_filter(
        "Users",
        &Filter::from(ordinary.clone()),
        &hostile.to_string()
    )
    .is_err());
    assert_eq!(
        update_with_filter("Users", &Filter::from(ordinary), &patch.to_string()).unwrap(),
        r#"mutation { update_Users(filter: {name: {_eq: "Alice"}}, input: {age: 31}) { _docID } }"#
    );
}

/// An unquoted collection or encrypted field name is a trust boundary too.
#[test]
fn a_hostile_collection_or_field_name_is_refused() {
    let doc = [json!({"name": "Alice"})];
    assert!(add("Users) { _docID } }", &doc, &AddOptions::default()).is_err());
    let options = AddOptions {
        encrypted_fields: vec!["name]) { _docID } }".into()],
        ..AddOptions::default()
    };
    assert!(add("Users", &doc, &options).is_err());
}

#[test]
fn encryption_is_asked_for_in_the_mutation() {
    let options = AddOptions {
        encrypt_doc: true,
        encrypted_fields: vec!["name".into(), "age".into()],
    };
    assert_eq!(
        add("Users", &[json!({"name": "Alice"})], &options).unwrap(),
        r#"mutation { add_Users(input: [{name: "Alice"}], encrypt: true, encryptFields: [name, age]) { _docID } }"#
    );
}

#[test]
fn a_filter_that_is_neither_an_object_nor_text_is_refused_with_go_text() {
    let error = delete_with_filter("Users", &Filter::from(json!(1))).unwrap_err();
    assert_eq!(error.to_string(), "unsupported filter type");
    let error = delete_with_filter("Users", &Filter::from(json!(""))).unwrap_err();
    assert_eq!(error.to_string(), "filter cannot be empty");
}

#[test]
fn an_updater_that_is_not_an_object_is_refused_with_go_text() {
    let filter = Filter::from(json!({"name": {"_eq": "Alice"}}));
    let error = update_with_filter("Users", &filter, "[1]").unwrap_err();
    assert_eq!(
        error.to_string(),
        "the updater of a document is of invalid type"
    );
}
