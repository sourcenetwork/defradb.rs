//! A GraphQL error's `extensions.code` reaches every Rust client, the CLI
//! included (#3288, `defra.cli.query.error_code`). Go reports no code.

use integration_test::actions::{AddCollection, Request};
use integration_test::scenario::Common;
use integration_test::test_case::TestCase;
use integration_test::ClientType;

const RUST_CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

fn unknown_collection_mutation(common: Common) -> TestCase {
    TestCase {
        client_types: Some(RUST_CLIENTS),
        ..TestCase::from_actions(vec![
            AddCollection {
                sdl: "type User { name: String }".into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Nope(input: {name: "x"}) { _docID } }"#.into(),
                expected_error: Some(
                    r#"parse error: Cannot query field "add_Nope" on type "Mutation"."#.into(),
                ),
                expected_error_code: Some("UNKNOWN_MUTATION_COLLECTION".into()),
                common,
                ..Default::default()
            }
            .into(),
        ])
    }
}

#[test]
#[mantra_macros::req_test("defra.cli.query.error_code")]
fn an_unknown_collection_mutation_carries_its_code() {
    unknown_collection_mutation(Common::default()).execute();
}

#[test]
#[mantra_macros::req_test("defra.cli.query.error_code")]
fn a_retried_unknown_collection_mutation_carries_its_code() {
    unknown_collection_mutation(Common {
        retry: true,
        ..Default::default()
    })
    .execute();
}
