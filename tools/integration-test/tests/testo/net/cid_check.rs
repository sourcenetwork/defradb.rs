//! A replicator push whose block does not hash to the CID it is pushed under
//! is refused, and the node stores no document from it (#3292). The same peer
//! then pushes an honest document, which merges, so the refused one is absent
//! because it was refused and not because the push never arrived.

use integration_test::actions::{
    AddCollection, GetCollections, NewNode, PushCommit, Request, Tamper,
};
use integration_test::scenario::{captures, Common, Eventually, Text, DEFAULT_NODE};
use integration_test::test_case::TestCase;
use serde_json::{json, Map, Value};

fn user(name: &str, age: i64) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(name));
    fields.insert("age".into(), json!(age));
    fields
}

fn push(fields: Map<String, Value>, tamper: Option<Tamper>, error: Option<&str>) -> PushCommit {
    PushCommit {
        target: DEFAULT_NODE.into(),
        collection_id: Text::reference("collection"),
        version_id: Text::reference("version"),
        fields,
        tamper,
        expected_error: error.map(String::from),
        ..Default::default()
    }
}

#[test]
#[mantra_macros::req_test("defra.p2p.cid_check")]
fn a_pushed_block_not_matching_its_cid_stores_no_document() {
    TestCase::from_actions(vec![
        NewNode {
            network: true,
            iroh: true,
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: "type User { name: String  age: Int }".into(),
            ..Default::default()
        }
        .into(),
        GetCollections {
            collection_name: Some("User".into()),
            filtered: true,
            common: Common {
                capture: captures([
                    ("collection", "/collections/0/CollectionID"),
                    ("version", "/collections/0/VersionID"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        push(
            user("Tampered", 1864),
            Some(Tamper {
                field: "age".into(),
                value: 1865,
            }),
            Some("block CID verification failed"),
        )
        .into(),
        push(user("Honest", 1), None, None).into(),
        Request {
            request: "query { User { name age } }".into(),
            expected_data: json!({ "User": [{ "name": "Honest", "age": 1 }] }),
            common: Common {
                eventually: Some(Eventually {
                    timeout_millis: Some(20_000),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}
