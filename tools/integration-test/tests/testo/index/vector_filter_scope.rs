use integration_test::actions::{AddCollection, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn top_level_fixture() -> TestCase {
    TestCase {
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: "type Note { title: String bucket: Int @index embedding: [Float32!] @index(vector: {dimensions: 3, flat: {metric: DOT}}) }".into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Note".into(),
                docs: Some(vec![
                    json!({"title": "outside", "bucket": 0, "embedding": [10, 0, 0]}),
                    json!({"title": "lower", "bucket": 1, "embedding": [2, 0, 0]}),
                    json!({"title": "higher", "bucket": 1, "embedding": [3, 0, 0]}),
                ]),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
}

fn no_warning() -> Value {
    m::any_of(json!([null, {}]))
}

fn score(dot: i32) -> Value {
    m::similarity_score("DOT", json!([dot, 0, 0]), json!([1, 0, 0]))
}

fn filter_warning() -> Value {
    json!({"warnings": [{
        "code": "VECTOR_INDEX_UNUSED",
        "detail": {"field": "embedding", "reason": "filter"},
        "message": "similarity query on field 'embedding' did not use the vector index"
    }]})
}

#[test]
#[mantra_macros::req_test("defra.query.simple.similarity.filtered_vector_warning")]
fn top_level_filter_uses_the_scalar_index() {
    let mut case = top_level_fixture();
    case.id = integration_test::scenario_id!();
    let query = "{ Note(filter: {bucket: {_eq: 1}}, order: {_alias: {sim: DESC}}, offset: 1, limit: 1) { title sim: SIMILARITY(embedding: {vector: [1, 0, 0]}) } }";
    case.actions.push(
        Request {
            request: query.into(),
            expected_data: json!({"Note": [{"title": "lower", "sim": score(2)}]}),
            expected_extensions: filter_warning(),
            ..Default::default()
        }
        .into(),
    );
    case.actions.push(
        Request {
            request: format!("query @explain(type: execute) {query}").into(),
            expected_data: m::explain_scan(json!({"indexFetches": 2, "docFetches": 2})),
            expected_extensions: filter_warning(),
            ..Default::default()
        }
        .into(),
    );
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.query.simple.similarity.filtered_vector_warning")]
fn empty_filter_keeps_vector_routing() {
    let mut case = top_level_fixture();
    case.id = integration_test::scenario_id!();
    case.actions.push(
        Request {
            request: "query @explain(type: execute) { Note(filter: {}, order: {_alias: {sim: DESC}}, limit: 1) { sim: SIMILARITY(embedding: {vector: [1, 0, 0]}) } }".into(),
            expected_data: m::explain_scan(json!({"indexFetches": 1, "docFetches": 1})),
            expected_extensions: no_warning(),
            ..Default::default()
        }
        .into(),
    );
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.query.simple.similarity.filtered_vector_warning")]
fn filter_warning_takes_precedence_over_missing_limit() {
    let mut case = top_level_fixture();
    case.id = integration_test::scenario_id!();
    case.actions.push(
        Request {
            request: "{ Note(filter: {bucket: {_eq: 1}}, order: {_alias: {sim: DESC}}) { title sim: SIMILARITY(embedding: {vector: [1, 0, 0]}) } }".into(),
            expected_data: json!({"Note": [{"title": "higher", "sim": score(3)}, {"title": "lower", "sim": score(2)}]}),
            expected_extensions: filter_warning(),
            ..Default::default()
        }
        .into(),
    );
    case.execute();
}

#[test]
#[mantra_macros::req_test("defra.query.simple.similarity.filtered_child_vector")]
fn scalar_filtered_children_page_per_parent() {
    let mut case = TestCase {
        id: integration_test::scenario_id!(),
        client_types: Some(CLIENTS),
        actions: vec![
            AddCollection {
                sdl: "type Owner { name: String notes: [Note] }
                      type Note { title: String keep: Boolean owner: Owner embedding: [Float32!] @index(vector: {dimensions: 3, flat: {metric: DOT}}) }".into(),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    };
    for (name, base) in [("a", 10), ("b", 1)] {
        case.actions.push(
            AddDoc {
                collection: "Owner".into(),
                doc_map: Some(json!({"name": name})),
                common: Common {
                    capture: captures([(name, "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
        for (suffix, keep, score) in [
            ("excluded", false, base + 2),
            ("first", true, base + 1),
            ("second", true, base),
        ] {
            let input = format!(
                r#"{{title: "{name}-{suffix}", keep: {keep}, embedding: [{score}, 0, 0], owner: ""#
            );
            case.actions.push(
                Request {
                    request: Text::concat([
                        "mutation { add_Note(input: ".into(),
                        input.into(),
                        Text::reference(name),
                        r#""}) { title } }"#.into(),
                    ]),
                    expected_data: json!({"add_Note": [{"title": format!("{name}-{suffix}")}]}),
                    ..Default::default()
                }
                .into(),
            );
        }
    }
    for (filter, first_page) in [
        (
            "{keep: {_eq: true}}",
            json!([{"title": "a-second", "sim": score(10)}]),
        ),
        ("{keep: {_eq: true}, _alias: {sim: {_lt: 11}}}", json!([])),
    ] {
        case.actions.push(
            Request {
                request: format!("{{ Owner(order: {{name: ASC}}) {{ name notes(filter: {filter}, order: {{_alias: {{sim: DESC}}}}, offset: 1, limit: 1) {{ title sim: SIMILARITY(embedding: {{vector: [1, 0, 0]}}) }} }} }}").into(),
                expected_data: json!({"Owner": [
                    {"name": "a", "notes": first_page},
                    {"name": "b", "notes": [{"title": "b-second", "sim": score(1)}]}
                ]}),
                expected_extensions: no_warning(),
                ..Default::default()
            }
            .into(),
        );
    }
    case.execute();
}
