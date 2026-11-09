//! The schema a GraphQL client introspects, checked against Go's schema
//! generator fixtures (`go.collection_version.introspection.full_schema`).
//!
//! `introspection_golden/*.gen.graphql` are Go's own fixtures, copied verbatim
//! from `internal/request/graphql/schema/testfixtures` at `GO_COMPAT_COMMIT`.
//! Go regenerates them with `make sdl-fixtures` and checks them against its
//! generator; a pin bump copies them again. Go is the only reference for now:
//! where defradb.rs's schema should differ, the reason goes in the requirement.
//!
//! `simple` checks the whole schema, which is what every collection shares. The
//! relation fixtures check only what a relation adds: their own types and the
//! root fields, since the shared types are those `simple` already checks.
//! Types named `__…` are left out: the GraphQL spec defines them, and each
//! GraphQL library words their descriptions its own way.

use integration_test::actions::{AddCollection, Request};
use integration_test::introspection_sdl;
use integration_test::m;
use integration_test::test_case::TestCase;
use serde_json::Value;

const QUERY: &str = include_str!("introspection_golden/client_introspection.gql");
const SIMPLE: &str = include_str!("introspection_golden/schema.simple.gen.graphql");

const ROOTS: [&str; 3] = ["Query", "Mutation", "Subscription"];

fn type_names(schema: &Value) -> Vec<String> {
    schema["types"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|ty| ty["name"].as_str().map(str::to_owned))
        .collect()
}

/// `fixture` as introspection describes it, less the spec's own types and,
/// when `base` is given, what `base` already checks.
fn expected(fixture: &str, base: Option<&str>) -> Value {
    let mut schema = introspection_sdl::introspection(fixture);
    let shared = base.map_or_else(Vec::new, |base| {
        type_names(&introspection_sdl::introspection(base))
    });
    if base.is_some() {
        schema["directives"] = Value::Array(Vec::new());
    }
    if let Some(types) = schema["types"].as_array_mut() {
        types.retain(|ty| {
            let name = ty["name"].as_str().unwrap_or_default();
            !name.starts_with("__") && (ROOTS.contains(&name) || !shared.iter().any(|s| s == name))
        });
    }
    serde_json::json!({ "__schema": schema })
}

fn introspects_as(sdl: &str, expected: Value) {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: sdl.into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: QUERY.into(),
            expected_data: m::same_schema(expected),
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
        expected(SIMPLE, None),
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
        expected(
            include_str!("introspection_golden/schema.relatedone.gen.graphql"),
            Some(SIMPLE),
        ),
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
        expected(
            include_str!("introspection_golden/schema.relatedmany.gen.graphql"),
            Some(SIMPLE),
        ),
    );
}
