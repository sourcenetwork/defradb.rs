//! Generated from the conformance scenarios of tests/integration/acp/dac/index/query_with_relation_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDACPolicy, AddDoc, PeerEventsStart, Request, WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_many_to_one_related_doc_with_identity_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {
                            author: {age: {_gt: 60}}
                        }) {
                            name
                            author {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Book": m::unordered(json!([
                        { "author": { "name": "Cornelia Funke" }, "name": "Theif Lord" },
                        { "author": { "name": "John Grisham" }, "name": "Painted House" },
                        {
                            "author": { "name": "John Grisham" },
                            "name": "A Time for Mercy",
                        },
                    ])),
                }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_many_to_one_related_doc_with_wrong_identity_should_not_fetch(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {
                            author: {age: {_gt: 60}}
                        }) {
                            name
                            author {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Book": [{ "author": { "name": "John Grisham" }, "name": "Painted House" }],
                }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_many_to_one_related_doc_without_identity_should_not_fetch()
{
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Book(filter: {
                            author: {age: {_gt: 60}}
                        }) {
                            name
                            author {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Book": [{ "author": { "name": "John Grisham" }, "name": "Painted House" }],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_one_to_many_related_doc_with_identity_should_fetch() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Author(filter: {
                            published: {rating: {_gt: 3}}
                        }) {
                            name
                            published {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Author": m::unordered(json!([
                        {
                            "name": "John Grisham",
                            "published": m::unordered(json!([
                                { "name": "Painted House" },
                                { "name": "A Time for Mercy" },
                            ])),
                        },
                        {
                            "name": "Cornelia Funke",
                            "published": m::unordered(json!([{ "name": "Theif Lord" }])),
                        },
                    ])),
                }),
                common: Common { identity_seed: Some(1), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_one_to_many_related_doc_with_wrong_identity_should_not_fetch(
) {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Author(filter: {
                            published: {rating: {_gt: 3}}
                        }) {
                            name
                            published {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John Grisham",
                            "published": [{ "name": "Painted House" }],
                        },
                    ],
                }),
                common: Common { identity_seed: Some(2), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.acp.dac.query.indexed")]
fn acp_with_index_upon_querying_private_one_to_many_related_doc_without_identity_should_not_fetch()
{
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            AddDACPolicy {
                policy: r#"
description: a test policy which marks a collection in a database as a resource
name: test
resources:
- name: author
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
- name: book
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - manages:
    - reader
    name: admin
    types:
    - actor
  - name: reader
    types:
    - actor
"#.into(),
                expected_data: json!({ "policyID": m::nonempty_string() }),
                common: Common { identity_seed: Some(1), capture: captures([("policy0", "/policyID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: Text::concat([Text::from(r#"
				type Author @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					age: Int @index
					verified: Boolean
					published: [Book]
				}

				type Book @policy(
					id: ""#), Text::reference("policy0"), Text::from(r#"",
					resource: "author"
				) {
					name: String
					rating: Float @index
					author: Author
				}"#)]),
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "John Grisham", "age": 65, "verified": true })),
                common: Common { retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "name": "Cornelia Funke", "age": 62, "verified": false })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc0_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "Painted House", "rating": 4.9 })),
                common: Common { retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_0"), "name": "A Time for Mercy", "rating": 4.5 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_1", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_1"),
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "_authorID": m::r("doc0_1"), "name": "Theif Lord", "rating": 4.8 })),
                common: Common { identity_seed: Some(1), retry: true, capture: captures([("doc1_2", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_2"),
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                        Author(filter: {
                            published: {rating: {_gt: 3}}
                        }) {
                            name
                            published {
                                name
                            }
                        }
                    }
                "#.into(),
                expected_data: json!({
                    "Author": [
                        {
                            "name": "John Grisham",
                            "published": [{ "name": "Painted House" }],
                        },
                    ],
                }),
                ..Default::default()
            }
            .into(),
        ],
        ..Default::default()
    }
    .execute();
}
