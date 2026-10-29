//! Generated from the conformance scenarios of tests/integration/net/peer_events/collection_test.go at defradb 137dc624052b.
//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    AddCollection, AddCollectionSubscription, ConnectPeers, DeleteCollectionSubscription,
    GetCollections, NewNode, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;

#[test]
#[mantra_macros::req_test("go.net.peer_events.collection")]
fn peer_events_multiple_nodes_subscribed_to_collection_should_receive_all_join_events() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db2".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db2".into()),
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
            AddCollection {
                sdl: r#"
                    type User {
                        name: String
                    }
                "#
                .into(),
                common: Common {
                    node: Some("db2".into()),
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
            AddCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db2".into()),
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
            ConnectPeers {
                target: "db2".into(),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: Text::reference("collection0"),
                    peers: vec!["db1".into(), "db2".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
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
#[mantra_macros::req_test("go.net.peer_events.collection")]
fn peer_events_on_subscribe_to_collection_should_receive_join_event_on_collection_topic() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
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
                topics: Some(vec![Topic {
                    topic: Text::reference("collection0"),
                    peers: vec!["db1".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
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
#[mantra_macros::req_test("go.net.peer_events.collection")]
fn peer_events_on_subscribe_to_multiple_collections_should_receive_join_events_on_all_topics() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
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
                    type Product {
                        title: String
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
                    type Product {
                        title: String
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
                    capture: captures([
                        ("collection0", "/collections/0/CollectionID"),
                        ("collection1", "/collections/1/CollectionID"),
                    ]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into(), "Product".into()],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into(), "Product".into()],
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
                topics: Some(vec![
                    Topic {
                        topic: Text::reference("collection0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("collection1"),
                        peers: vec!["db1".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
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
#[mantra_macros::req_test("go.net.peer_events.collection")]
fn peer_events_on_unsubscribe_from_collection_should_receive_left_event() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
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
                topics: Some(vec![Topic {
                    topic: Text::reference("collection0"),
                    peers: vec!["db1".into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteCollectionSubscription {
                collections: vec!["User".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: Text::reference("collection0"),
                    peers: vec!["db1".into()],
                }]),
                event_type: Some(EventType::Left),
                millis: Some(5000),
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
#[mantra_macros::req_test("go.net.peer_events.collection")]
fn peer_events_on_unsubscribe_from_multiple_collections_should_receive_left_events() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![
            NewNode {
                network: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            NewNode {
                network: true,
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                start: true,
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
                    type Product {
                        title: String
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
                    type Product {
                        title: String
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
                    capture: captures([
                        ("collection0", "/collections/0/CollectionID"),
                        ("collection1", "/collections/1/CollectionID"),
                    ]),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into(), "Product".into()],
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            AddCollectionSubscription {
                collections: vec!["User".into(), "Product".into()],
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
                topics: Some(vec![
                    Topic {
                        topic: Text::reference("collection0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("collection1"),
                        peers: vec!["db1".into()],
                    },
                ]),
                event_type: Some(EventType::Joined),
                millis: Some(5000),
                common: Common {
                    node: Some("db0".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            DeleteCollectionSubscription {
                collections: vec!["User".into(), "Product".into()],
                common: Common {
                    node: Some("db1".into()),
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
            WaitForPeersEvents {
                topics: Some(vec![
                    Topic {
                        topic: Text::reference("collection0"),
                        peers: vec!["db1".into()],
                    },
                    Topic {
                        topic: Text::reference("collection1"),
                        peers: vec!["db1".into()],
                    },
                ]),
                event_type: Some(EventType::Left),
                millis: Some(5000),
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
