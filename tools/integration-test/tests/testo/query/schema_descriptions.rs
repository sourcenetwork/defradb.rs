//! The GraphQL schema describes itself to clients with Go DefraDB's texts
//! (#2106, `defra.request.schema_descriptions`).

use integration_test::actions::{AddCollection, Request};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("defra.request.schema_descriptions")]
fn introspection_shows_go_descriptions() {
    TestCase::from_actions(vec![
        AddCollection {
            sdl: "type Book { title: String }".into(),
            ..Default::default()
        }
        .into(),
        Request {
            request: r#"query {
                commit: __type(name: "Commit") { description }
                ordering: __type(name: "Ordering") { enumValues { name description } }
                operators: __type(name: "StringOperatorBlock") { description }
            }"#
            .into(),
            expected_data: json!({
                "commit": { "description": "\nCommit represents an individual commit to a MerkleCRDT, every mutation to a\n document will result in a new commit per modified field, and one composite\n commit composed of the field level commits and, in the case of an update,\n the prior composite commit.  If the collection is branchable, there will\n also be a collection-level commit for each mutation.\n" },
                "ordering": { "enumValues": [
                    { "name": "ASC", "description": "\nSort the results in ascending order, e.g. null,1,2,3,a,b,c.\n" },
                    { "name": "DESC", "description": "\nSort the results in descending order, e.g. c,b,a,3,2,1,null.\n" },
                ] },
                "operators": { "description": "\nThese are the set of filter operators available for use when filtering on String\n values.\n" },
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
