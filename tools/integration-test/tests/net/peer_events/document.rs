//! Generated from the conformance scenarios of tests/integration/net/peer_events/document_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, AddDoc, AddDocumentSubscription, ConnectPeers,
    DeleteDocumentSubscription, GetCollections, NodeOpenP2P, PeerEventsStart, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::json;

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_all_topic_types_should_receive_join_events_on_all() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![
                    Topic {
                        topic: "doc-sync".into(),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("collection0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("doc0"),
                        peers: vec!["db1".into()],
                    },
                ],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_document_and_doc_sync_topics_should_receive_join_events_on_both() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![
                    Topic {
                        topic: "doc-sync".into(),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("doc0"),
                        peers: vec!["db1".into()],
                    },
                ],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_on_subscribe_to_document_should_receive_join_event_on_document_topic() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![Topic {
                    topic: Text::reference("doc0"),
                    peers: vec!["db1".into()],
                }],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_on_subscribe_to_multiple_documents_should_receive_join_events_on_all_topics() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![
                    Topic {
                        topic: Text::reference("doc0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("doc1"),
                        peers: vec!["db1".into()],
                    },
                ],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_on_unsubscribe_from_document_should_receive_left_event() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![Topic {
                    topic: Text::reference("doc0"),
                    peers: vec!["db1".into()],
                }],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDocumentSubscription {
                doc_ids: vec![Text::reference("doc0")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![Topic {
                    topic: Text::reference("doc0"),
                    peers: vec!["db1".into()],
                }],
                event_type: EventType::Left,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}

#[test]
#[mantra_macros::req_test("go.net.peer_events.document")]
fn peer_events_on_unsubscribe_from_multiple_documents_should_receive_left_events() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NodeOpenP2P {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NodeOpenP2P {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            PeerEventsStart {
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            GetCollections {
                common: Common {
                    node: Some("db0".into()),
                    capture: captures([("collection0", "/collections/0/CollectionID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc0", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "John" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    node: Some("db0".into()),
                    retry: true,
                    capture: captures([("doc1", "/docID")]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDoc {
                collection: "User".into(),
                doc_map: Some(json!({ "name": "Alice" })),
                common: Common {
                    node: Some("db1".into()),
                    retry: true,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddDocumentSubscription {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            ConnectPeers {
                target: "db1".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![
                    Topic {
                        topic: Text::reference("doc0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("doc1"),
                        peers: vec!["db1".into()],
                    },
                ],
                event_type: EventType::Joined,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteDocumentSubscription {
                doc_ids: vec![Text::reference("doc0"), Text::reference("doc1")],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: vec![
                    Topic {
                        topic: Text::reference("doc0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("doc1"),
                        peers: vec!["db1".into()],
                    },
                ],
                event_type: EventType::Left,
                millis: 5000,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        ],
        without_db: true,
        ..Default::default()
    }
    .execute();
}
