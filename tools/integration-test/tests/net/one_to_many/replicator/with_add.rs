//! Generated from the conformance scenarios of tests/integration/net/one_to_many/replicator/with_add_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::{
    AddCollection, AddDoc, AddReplicator, NodeOpenP2P, PeerEventsStart, Request, WaitForMerge,
    WaitForUpdate,
};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.relations")]
fn p2_p_one_to_many_replicator() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
                        Name: String
                        Books: [Book]
                    }
                    type Book {
                        Name: String
                        Author: Author
                    }
                "#.into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type Author {
                        Name: String
                        Books: [Book]
                    }
                    type Book {
                        Name: String
                        Author: Author
                    }
                "#.into(),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddReplicator {
                target: "db1".into(),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Author".into(),
                doc_map: Some(json!({ "Name": "Saadi" })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc0_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc0_0"),
                common: Common { node: Some("db0".into()), capture: captures([("head0", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "Book".into(),
                doc_map: Some(json!({ "Name": "Gulistan", "_AuthorID": m::r("doc0_0") })),
                common: Common { node: Some("db0".into()), retry: true, capture: captures([("doc1_0", "/docID")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForUpdate {
                doc_id: Text::reference("doc1_0"),
                common: Common { node: Some("db0".into()), capture: captures([("head1", "/cid")]), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc0_0"),
                cid: Text::reference("head0"),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            WaitForMerge {
                doc_id: Text::reference("doc1_0"),
                cid: Text::reference("head1"),
                expected_data: json!({}),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            Name
                                            Author {
                                                Name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "Author": { "Name": "Saadi" }, "Name": "Gulistan" }] }),
                common: Common { node: Some("db0".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
            Request {
                request: r#"
                    query {
                                        Book {
                                            Name
                                            Author {
                                                Name
                                            }
                                        }
                                    }
                "#.into(),
                expected_data: json!({ "Book": [{ "Author": { "Name": "Saadi" }, "Name": "Gulistan" }] }),
                common: Common { node: Some("db1".into()), ..Default::default() },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
