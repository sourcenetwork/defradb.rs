use serde_json::{json, Value as JsonValue};

use super::split::names_alias;
use super::Filter;

fn filter(conditions: JsonValue) -> Filter {
    Filter::from_conditions(conditions.as_object().unwrap().clone())
}

fn json_of(filter: &Option<Filter>) -> Option<JsonValue> {
    filter
        .as_ref()
        .map(|f| JsonValue::Object(f.conditions().clone()))
}

#[test]
fn names_alias_finds_an_alias_at_any_depth() {
    assert!(names_alias(&json!({"_alias": {"sim": {"_gt": 1}}})));
    assert!(names_alias(
        &json!({"_or": [{"a": {"_eq": 1}}, {"_alias": {"sim": {"_gt": 1}}}]})
    ));
    assert!(names_alias(
        &json!({"_not": {"_alias": {"sim": {"_gt": 1}}}})
    ));
    assert!(!names_alias(
        &json!({"_or": [{"a": {"_eq": 1}}, {"b": {"_eq": 2}}]})
    ));
}

#[test]
fn a_top_level_alias_is_split_off() {
    let f = filter(json!({"a": {"_eq": 1}, "_alias": {"sim": {"_gt": 1}}}));
    let (row, alias) = f.split_for_scores();
    assert_eq!(json_of(&row), Some(json!({"a": {"_eq": 1}})));
    assert_eq!(
        json_of(&alias),
        Some(json!({"_alias": {"sim": {"_gt": 1}}}))
    );
}

#[test]
fn an_alias_under_a_combinator_makes_the_whole_filter_wait() {
    for conditions in [
        json!({"_or": [{"a": {"_eq": 1}}, {"_alias": {"sim": {"_gt": 1}}}]}),
        json!({"_and": [{"a": {"_eq": 1}}, {"_alias": {"sim": {"_gt": 1}}}]}),
        json!({"_not": {"_alias": {"sim": {"_gt": 1}}}}),
    ] {
        let f = filter(conditions.clone());
        let (row, alias) = f.split_for_scores();
        assert!(row.is_none());
        assert_eq!(json_of(&alias), Some(conditions));
    }
}

#[test]
fn a_filter_without_an_alias_runs_before_scores() {
    let conditions = json!({"_or": [{"a": {"_eq": 1}}, {"b": {"_eq": 2}}]});
    let (row, alias) = filter(conditions.clone()).split_for_scores();
    assert_eq!(json_of(&row), Some(conditions));
    assert!(alias.is_none());
}

#[test]
fn waits_for_scores_agrees_with_the_split() {
    for conditions in [
        json!({"_alias": {"sim": {"_gt": 1}}}),
        json!({"a": {"_eq": 1}, "_alias": {"sim": {"_gt": 1}}}),
        json!({"_or": [{"a": {"_eq": 1}}, {"_alias": {"sim": {"_gt": 1}}}]}),
        json!({"_not": {"_alias": {"sim": {"_gt": 1}}}}),
        json!({"a": {"_eq": 1}}),
        json!({"_or": [{"a": {"_eq": 1}}, {"b": {"_eq": 2}}]}),
    ] {
        let f = filter(conditions.clone());
        assert_eq!(
            f.waits_for_scores(),
            f.split_for_scores().1.is_some(),
            "{conditions}"
        );
    }
}

#[test]
fn an_empty_filter_does_not_wait_for_scores() {
    assert!(!filter(json!({})).waits_for_scores());
}
