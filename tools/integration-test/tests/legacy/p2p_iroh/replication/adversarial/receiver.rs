//! The real node on the receiving end, and what can be read back from it.

use std::time::Duration;

use integration_test::{extract_p2p_addr, poll_until, DefraClient, TestCluster};
use serde_json::Value;

pub const SCHEMA: &str = "type User { name: String  age: Int }";

const TIMEOUT: Duration = Duration::from_secs(20);

pub struct Collection {
    pub collection_id: String,
    pub version_id: String,
}

pub async fn wait_listening(cluster: &TestCluster, count: usize) {
    for index in 0..count {
        cluster
            .wait_for_log(index, "p2p_listening", TIMEOUT)
            .await
            .unwrap_or_else(|_| panic!("node{index} P2P listener did not start"));
    }
}

/// One iroh node with the plain `User` collection.
pub async fn start_public() -> (TestCluster, Vec<String>, Collection) {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .with_iroh_transport()
        .build()
        .await
        .expect("node starts");
    wait_listening(&cluster, 1).await;
    let client = cluster.client(0);
    client.schema_add(SCHEMA).expect("schema deploys");
    let collection = describe(&client, "User");
    let addrs = p2p_addrs(&cluster, 0);
    (cluster, addrs, collection)
}

/// Every address the node reports. The first may be a public address that
/// loopback cannot hairpin to, so a local peer needs the whole list.
pub fn p2p_addrs(cluster: &TestCluster, index: usize) -> Vec<String> {
    let info = cluster.client(index).p2p_info().expect("p2p info");
    let addrs: Vec<String> = info
        .as_array()
        .map(|addrs| {
            addrs
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if addrs.is_empty() {
        vec![extract_p2p_addr(cluster, index)]
    } else {
        addrs
    }
}

pub fn describe(client: &DefraClient, name: &str) -> Collection {
    let described = client.collection_describe_version(name).expect("describe");
    let found = find_key(&described, "CollectionID")
        .zip(find_key(&described, "VersionID"))
        .unwrap_or_else(|| panic!("no collection IDs in {described}"));
    Collection {
        collection_id: found.0,
        version_id: found.1,
    }
}

fn find_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| map.values().find_map(|child| find_key(child, key))),
        Value::Array(items) => items.iter().find_map(|child| find_key(child, key)),
        _ => None,
    }
}

pub fn users(client: &DefraClient, identity: Option<&str>) -> Vec<Value> {
    let query = "query { User { _docID name age } }";
    let result = match identity {
        Some(key) => client.query_with_identity(query, key),
        None => client.query(query),
    };
    result.unwrap_or_default()["User"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

pub fn user_by_id(client: &DefraClient, identity: Option<&str>, doc_id: &str) -> Option<Value> {
    users(client, identity)
        .into_iter()
        .find(|row| row["_docID"].as_str() == Some(doc_id))
}

/// Wait until the document reads back satisfying `accept`.
pub async fn wait_for_user(
    client: &DefraClient,
    identity: Option<&str>,
    doc_id: &str,
    accept: impl Fn(&Value) -> bool,
) {
    poll_until(
        || user_by_id(client, identity, doc_id).is_some_and(|row| accept(&row)),
        TIMEOUT,
        Duration::from_millis(200),
        &format!("document {doc_id} did not arrive in the expected state"),
    )
    .await;
}
