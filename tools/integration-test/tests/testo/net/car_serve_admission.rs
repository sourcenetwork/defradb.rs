//! A full CAR-serve lane holds requests for a bounded wait, then hands each
//! freed slot to a waiting spoke. With a one-serve hub, several spokes catch up
//! at once and all converge while the hub reports that serves waited (#2536).

use std::collections::BTreeMap;

use integration_test::actions::shared::{EventType, Topic};
use integration_test::actions::{
    Actions, AddCollection, AddCollectionSubscription, AddDoc, AdmissionSpec, ConnectPeers,
    GetCollections, NewNode, Request, SyncCounters, UpdateDoc, WaitForPeersEvents,
};
use integration_test::scenario::{captures, Common, Eventually, Text};
use integration_test::test_case::TestCase;
use integration_test::ClientType;
use serde_json::{json, Value};

const HUB: &str = "hub";
const SPOKES: [&str; 4] = ["spoke0", "spoke1", "spoke2", "spoke3"];
const DOCS: usize = 6;
const HISTORY: usize = 16;
const SDL: &str = "type Users { name: String }";

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn counters(limits: &[(&str, u64)]) -> BTreeMap<String, u64> {
    limits
        .iter()
        .map(|(name, limit)| ((*name).to_owned(), *limit))
        .collect()
}

fn node(name: &str, admission: Option<AdmissionSpec>) -> Vec<Actions> {
    vec![
        NewNode {
            network: true,
            admission,
            common: on(name),
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: SDL.into(),
            common: on(name),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            common: on(name),
            ..Default::default()
        }
        .into(),
    ]
}

fn add_doc(index: usize) -> Actions {
    AddDoc {
        collection: "Users".into(),
        doc_map: Some(json!({ "name": format!("doc-{index}-0") })),
        common: Common {
            retry: true,
            capture: captures([(&format!("doc{index}"), "/docID")]),
            ..on(HUB)
        },
        ..Default::default()
    }
    .into()
}

fn update_doc(index: usize, revision: usize) -> Actions {
    UpdateDoc {
        collection: "Users".into(),
        doc_id: Text::reference(&format!("doc{index}")),
        doc_map: json!({ "name": format!("doc-{index}-{revision}") }),
        common: Common {
            retry: true,
            ..on(HUB)
        },
        ..Default::default()
    }
    .into()
}

fn spoke_has_every_document(spoke: &str) -> Actions {
    let expected: Vec<Value> = (0..DOCS)
        .map(|index| json!({ "name": format!("doc-{index}-{HISTORY}") }))
        .collect();
    Request {
        request: "query { Users(order: {name: ASC}) { name } }".into(),
        expected_data: json!({ "Users": expected }),
        common: Common {
            node: Some(spoke.into()),
            eventually: Some(Eventually {
                timeout_millis: Some(120_000),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.p2p.car_serve_lane")]
fn concurrent_spokes_catch_up_through_a_single_serve_lane() {
    let mut actions = node(
        HUB,
        Some(AdmissionSpec {
            max_pending_dags: 1_000,
            rate_limit_burst: 500,
            rate_limit_rate: 50.0,
            max_active_car_serves: Some(1),
            deferral_peer_bytes: None,
        }),
    );
    for spoke in SPOKES {
        actions.extend(node(spoke, None));
    }

    actions.push(
        GetCollections {
            collection_name: Some("Users".into()),
            filtered: true,
            common: Common {
                capture: captures([("collection", "/collections/0/CollectionID")]),
                ..on(HUB)
            },
            ..Default::default()
        }
        .into(),
    );
    for index in 0..DOCS {
        actions.push(add_doc(index));
    }
    for revision in 1..HISTORY {
        for index in 0..DOCS {
            actions.push(update_doc(index, revision));
        }
    }

    for name in std::iter::once(HUB).chain(SPOKES) {
        actions.push(
            AddCollectionSubscription {
                collections: vec!["Users".into()],
                common: on(name),
                ..Default::default()
            }
            .into(),
        );
    }
    for spoke in SPOKES {
        actions.push(
            ConnectPeers {
                target: HUB.into(),
                common: on(spoke),
                ..Default::default()
            }
            .into(),
        );
        actions.push(
            WaitForPeersEvents {
                topics: Some(vec![Topic {
                    topic: Text::reference("collection"),
                    peers: vec![HUB.into()],
                }]),
                event_type: Some(EventType::Joined),
                millis: Some(10_000),
                common: on(spoke),
                ..Default::default()
            }
            .into(),
        );
    }

    // Each update reaches every spoke together. Their CAR requests queue on
    // the hub's one-serve lane while the fetched histories are still walking.
    for index in 0..DOCS {
        actions.push(update_doc(index, HISTORY));
    }
    for spoke in SPOKES {
        actions.push(spoke_has_every_document(spoke));
    }
    actions.push(
        SyncCounters {
            at_least: counters(&[
                ("request_dispatch/recovery_capacity", 1),
                ("request_dispatch/recovery_waited_total", 1),
            ]),
            at_most: counters(&[("request_dispatch/recovery_capacity", 1)]),
            common: Common {
                eventually: Some(Eventually::default()),
                ..on(HUB)
            },
            ..Default::default()
        }
        .into(),
    );

    TestCase {
        id: integration_test::scenario_id!(),
        actions: actions.into_iter().map(Into::into).collect(),
        client_types: Some(&[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli]),
        ..Default::default()
    }
    .execute();
}
