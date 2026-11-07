//! The whole schema a GraphQL client introspects, for Go's three schema
//! generator fixtures (`internal/request/graphql/schema/testfixtures`),
//! compared against a golden response (`go.collection_version.introspection.full_schema`).
//!
//! The goldens in `introspection_golden/` are the response to
//! `client_introspection.gql` (the Altair/GraphiQL/Postman query Go's client
//! introspection tests use), taken from Go DefraDB at `GO_COMPAT_COMMIT`. Go is
//! the only reference for now: where defradb.rs's schema should differ, the
//! golden is edited in the same change and the reason recorded in the
//! requirement. Lists compare in any order, since Go's follow map iteration.

use integration_test::actions::{AddCollection, Request};
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::Value;

const QUERY: &str = include_str!("introspection_golden/client_introspection.gql");

fn introspects_as(sdl: &str, golden: &str) {
    let golden: Value = serde_json::from_str(golden).expect("golden is JSON");
    TestCase::from_actions(vec![
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: QUERY.into(),
            expected_data: m::same_schema(golden),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.full_schema")]
fn introspection_matches_golden_simple() {
    introspects_as(
        "type User {
            name: String
            age: Int
            verified: Boolean
            points: Int @crdt(type: pncounter)
        }",
        include_str!("introspection_golden/simple.json"),
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.full_schema")]
fn introspection_matches_golden_related_one() {
    introspects_as(
        "type Book {
            name: String
            rating: Float
            author: Author @primary
        }
        type Author {
            name: String
            age: Int
            verified: Boolean
            published: Book
        }",
        include_str!("introspection_golden/related_one.json"),
    );
}

#[test]
#[mantra_macros::req_test("go.collection_version.introspection.full_schema")]
fn introspection_matches_golden_related_many() {
    introspects_as(
        "type Book {
            name: String
            rating: Float
            author: Author
        }
        type Author {
            name: String
            age: Int
            verified: Boolean
            published: [Book]
        }",
        include_str!("introspection_golden/related_many.json"),
    );
}
