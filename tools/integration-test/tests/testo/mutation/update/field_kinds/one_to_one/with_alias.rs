//! Generated from the conformance scenarios of tests/integration/mutation/update/field_kinds/one_to_one/with_alias_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{AddCollection, AddDoc, Request, UpdateDoc};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_one_unique")]
fn mutation_update_one_to_one_alias_relation_name_to_link_from_primary_side() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "published": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "New Shahzad" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Author".into(),
                doc_id: Text::reference("doc1_1"),
                doc_map: json!({ "published": m::r("doc0_0") }),
                expected_error: Some(
                    "can not index a doc's field(s) that violates unique index".into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_one_secondary")]
fn mutation_update_one_to_one_alias_relation_name_to_link_from_secondary_side_collection_api() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "published": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "New Shahzad" })),
                common: Common {
                    capture: captures([("doc1_1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Book".into(),
                doc_id: Text::reference("doc0_0"),
                doc_map: json!({ "author": m::r("doc1_1") }),
                expected_error: Some("cannot set relation from secondary side".into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.relation.one_to_one_secondary")]
fn mutation_update_one_to_one_alias_relation_name_to_link_from_secondary_side_gql() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Book(input: {name: "Painted House"}) { _docID } }"#
                    .into(),
                common: Common {
                    capture: captures([("doc0_0", "/data/add_Book/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(
                        r#"mutation { add_Author(input: {name: "John Grisham", published: ""#,
                    ),
                    Text::reference("doc0_0"),
                    Text::from(r#""}) { _docID } }"#),
                ]),
                common: Common {
                    capture: captures([("doc1_0", "/data/add_Author/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"mutation { add_Author(input: {name: "New Shahzad"}) { _docID } }"#
                    .into(),
                common: Common {
                    capture: captures([("doc1_1", "/data/add_Author/0/_docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            Request {
                request: Text::concat([
                    Text::from(r#"mutation { update_Book(docID: ""#),
                    Text::reference("doc0_0"),
                    Text::from(r#"", input: {author: ""#),
                    Text::reference("doc1_1"),
                    Text::from(r#""}) { _docID } }"#),
                ]),
                expected_error: Some(r#"Argument "input" has invalid value"#.into()),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.mutation.relation.invalid_id")]
fn mutation_update_one_to_one_alias_with_invalid_length_relation_id_to_link_error() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddCollection {
                sdl: r#"
                    type Book {
                        name: String
                        rating: Float
                        author: Author
                    }

                    type Author {
                        name: String
                        age: Int
                        verified: Boolean
                        published: Book @primary
                    }
                "#
                .into(),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "name": "Painted House" })),
                common: Common {
                    capture: captures([("doc0_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "published": m::r("doc0_0") })),
                common: Common {
                    capture: captures([("doc1_0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            UpdateDoc {
                collection: "Author".into(),
                doc_id: Text::reference("doc1_0"),
                doc_map: json!({ "published": "bae-35953ca-518d-9e6b-9ce6cd00eff5" }),
                expected_error: Some(
                    r#"uuid: incorrect UUID length 30 in string "35953ca-518d-9e6b-9ce6cd00eff5""#
                        .into(),
                ),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
