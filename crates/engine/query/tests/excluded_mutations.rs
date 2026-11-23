//! Mutations `@skip` or `@include` leave out: parsed once per fragment and
//! held to the same limits as those that run
//! (`defra.query.filter.input-validation`).

use query::query_parse::{parse_request_with_limits, ParsedOperation};
use query::QueryLimits;

fn parse(request: &str, limits: QueryLimits) -> query::Result<(usize, usize)> {
    match parse_request_with_limits(request, None, None, limits)? {
        ParsedOperation::Mutation {
            mutations,
            excluded,
            ..
        } => Ok((mutations.len(), excluded.len())),
        _ => panic!("a mutation operation"),
    }
}

/// Two left-out spreads of one fragment expand it once, so reconverging
/// fragments cost one expansion per definition, not one per path.
#[test]
fn a_fragment_left_out_twice_is_expanded_once() {
    let request = r#"
        mutation { ...G @skip(if: true) }
        fragment G on Mutation { ...F ...F }
        fragment F on Mutation { delete_Author(filter: {}) { _docID } }
    "#;
    assert_eq!(parse(request, QueryLimits::default()).unwrap(), (0, 1));
}

/// A doubling chain of left-out fragments stays linear in its length.
#[test]
fn reconverging_left_out_fragments_stay_linear() {
    let mut request = String::from("mutation { ...F0 @skip(if: true) }\n");
    for level in 0..20 {
        let next = level + 1;
        request.push_str(&format!(
            "fragment F{level} on Mutation {{ ...F{next} ...F{next} }}\n"
        ));
    }
    request.push_str("fragment F20 on Mutation { delete_Author(filter: {}) { _docID } }\n");
    assert_eq!(parse(&request, QueryLimits::default()).unwrap(), (0, 1));
}

/// A left-out visit never stops a later kept spread of the same fragment
/// from running.
#[test]
fn a_kept_spread_after_a_left_out_one_still_runs() {
    let request = r#"
        mutation { ...F @skip(if: true) ...F }
        fragment F on Mutation { delete_Author(filter: {}) { _docID } }
    "#;
    assert_eq!(parse(request, QueryLimits::default()).unwrap(), (1, 1));
}

/// The filter depth limit holds for a left-out mutation as for one that
/// runs, since its filter is still walked.
#[test]
fn a_left_out_mutation_is_held_to_the_filter_depth_limit() {
    let limits = QueryLimits {
        max_filter_depth: 1,
        ..QueryLimits::default()
    };
    let field = r#"delete_Author(filter: {_and: [{_and: [{name: {_eq: "x"}}]}]})"#;
    let kept = parse(&format!("mutation {{ {field} {{ _docID }} }}"), limits).unwrap_err();
    let skipped = parse(
        &format!("mutation {{ {field} @skip(if: true) {{ _docID }} }}"),
        limits,
    )
    .unwrap_err();
    assert_eq!(skipped.to_string(), kept.to_string());
}

/// The selection depth limit holds for a left-out mutation too.
#[test]
fn a_left_out_mutation_is_held_to_the_selection_depth_limit() {
    let limits = QueryLimits {
        max_query_depth: 2,
        ..QueryLimits::default()
    };
    let selection = "{ books { author { books { title } } } }";
    let kept = parse(&format!("mutation {{ delete_Author {selection} }}"), limits).unwrap_err();
    let skipped = parse(
        &format!("mutation {{ delete_Author @skip(if: true) {selection} }}"),
        limits,
    )
    .unwrap_err();
    assert_eq!(skipped.to_string(), kept.to_string());
}
