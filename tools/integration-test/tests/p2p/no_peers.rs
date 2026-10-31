//! A document pushed with no peer subscribed to its collection's topic emits
//! `P2PNoPeers`, as Go Shinzo's publish does when it finds no peers (#2441).

use integration_test::actions::{AddCollection, AddDoc, NewNode, WaitForPeersEvents};
use integration_test::test_case::TestCase;
use serde_json::json;

/// `User`'s collection ID, which is also its collection topic.
const USER_COLLECTION: &str = "bafyreibhpgygzsmki22sql5ejzcojrrxbc5iuhpydhdzxul5w2znc7zrgu";

#[test]
fn a_push_with_no_peers_says_so() {
    TestCase::from_actions(vec![
        NewNode {
            network: true,
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type User { name: String }".into(),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            start: true,
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "User".into(),
            doc_map: Some(json!({ "name": "John" })),
            ..Default::default()
        }
        .into(),
        WaitForPeersEvents {
            no_peers: true,
            expected_data: json!({
                "docID": "bae-0e9f018a-9ca4-5ae2-879c-f607840c6295",
                "collectionID": USER_COLLECTION,
                "topic": USER_COLLECTION,
            }),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
