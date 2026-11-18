use query::query_parse::parse_mutations_with_variables;
use rapidhash::RapidHashMap;
use serde_json::json;

#[test]
fn mutation_inputs_reject_non_objects() {
    for value in [
        json!(null),
        json!([]),
        json!([{}]),
        json!(42),
        json!(true),
        json!("patch"),
    ] {
        let variables: RapidHashMap<_, _> = [("input".to_owned(), value)].into_iter().collect();
        for query in [
            "mutation($input: ProbeMutationInputArg) { update_Probe(input: $input) { label } }",
            "mutation($input: ProbeMutationInputArg) { upsert_Probe(filter: {}, add: $input, update: {}) { label } }",
            "mutation($input: ProbeMutationInputArg) { upsert_Probe(filter: {}, add: {}, update: $input) { label } }",
        ] {
            assert!(parse_mutations_with_variables(query, Some(&variables)).is_err(), "{query}: {variables:?}");
        }
    }
}

#[test]
fn missing_input_variable_is_an_error() {
    let query = "mutation($input: ProbeMutationInputArg) { update_Probe(input: $input) { label } }";
    assert!(parse_mutations_with_variables(query, None).is_err());
}

#[test]
fn malformed_inline_input_is_an_error() {
    for input in ["[]", "42", "true", "\"patch\""] {
        let query = format!("mutation {{ update_Probe(input: {input}) {{ label }} }}");
        assert!(
            parse_mutations_with_variables(&query, None).is_err(),
            "{query}"
        );
    }
}

#[test]
fn default_object_input_is_resolved() {
    let parsed = parse_mutations_with_variables(
        "mutation($input: ProbeMutationInputArg = {label: \"default\"}) { update_Probe(input: $input) { label } }",
        None,
    ).unwrap();
    assert_eq!(parsed[0].update_input["label"], json!("default"));
}
