//! Which blocks LWW and counter writes produce, and the less common field
//! kinds: block CIDs are pinned, so a change to the encoding shows up here.

use integration_test::{DefraClient, TestCluster};
use serde_json::Value;

const SDL: &str = "type Item {
    label: String
    name: String
    n: Int @crdt(type: pncounter)
    f: Float @crdt(type: pcounter)
    g: Float32 @crdt(type: pncounter)
    hits: Int @crdt(type: pcounter)
}";

async fn node() -> TestCluster {
    let cluster = TestCluster::builder()
        .rust_nodes(1)
        .no_signing_multiplier()
        .build()
        .await
        .unwrap();
    cluster.client(0).schema_add(SDL).expect("schema add");
    cluster
}

fn create(node: &DefraClient, input: &str) -> String {
    node.query(&format!(
        "mutation {{ add_Item(input: {input}) {{ _docID }} }}"
    ))
    .expect("add_Item")["add_Item"][0]["_docID"]
        .as_str()
        .expect("_docID")
        .to_string()
}

fn update(node: &DefraClient, id: &str, input: &str) {
    node.query(&format!(
        r#"mutation {{ update_Item(docID: "{id}", input: {input}) {{ _docID }} }}"#
    ))
    .expect("update_Item");
}

fn row(node: &DefraClient, id: &str) -> Value {
    let data = node
        .query(&format!(
            r#"query {{ Item(docID: "{id}") {{ name n f g hits }} }}"#
        ))
        .expect("query Item");
    data["Item"][0].clone()
}

fn commits(node: &DefraClient, id: &str) -> Vec<String> {
    let data = node
        .query(&format!(
            r#"query {{ _commits(docID: "{id}") {{ cid fieldName height }} }}"#
        ))
        .expect("query _commits");
    let mut lines: Vec<String> = data["_commits"]
        .as_array()
        .expect("_commits rows")
        .iter()
        .map(|c| {
            format!(
                "{} {} {}",
                c["height"],
                c["fieldName"].as_str().unwrap_or("-"),
                c["cid"].as_str().expect("cid")
            )
        })
        .collect();
    lines.sort();
    lines
}

#[nuclei::test]
async fn create_and_update_blocks_are_pinned() {
    edge::compat(async move {
        let cluster = node().await;
        let node = cluster.client(0);
        let id = create(&node, r#"{label: "a", name: "first", hits: 2}"#);
        assert_eq!(commits(&node, &id), EXPECTED_CREATE);

        update(&node, &id, r#"{name: "second", hits: 3}"#);
        let mut after: Vec<String> = commits(&node, &id)
            .into_iter()
            .map(|line| match line.split_once(' ') {
                Some(("2", rest)) if !rest.starts_with("name ") => {
                    format!("2 {} <random>", rest.split(' ').next().unwrap_or_default())
                }
                _ => line,
            })
            .collect();
        after.sort();
        assert_eq!(after, EXPECTED_UPDATE);
    })
    .await;
}

#[nuclei::test]
async fn a_zero_increment_writes_a_block() {
    edge::compat(async move {
        let cluster = node().await;
        let node = cluster.client(0);
        let id = create(&node, r#"{label: "a", hits: 2}"#);
        let before = commits(&node, &id).len();

        update(&node, &id, "{hits: 0}");
        assert_eq!(row(&node, &id)["hits"], 2);
        assert_eq!(commits(&node, &id).len(), before + ZERO_INCREMENT_BLOCKS);
    })
    .await;
}

#[nuclei::test]
async fn a_same_value_lww_rewrite_writes_no_block() {
    edge::compat(async move {
        let cluster = node().await;
        let node = cluster.client(0);
        let id = create(&node, r#"{label: "a", name: "x"}"#);
        let before = commits(&node, &id).len();

        update(&node, &id, r#"{name: "x"}"#);
        assert_eq!(row(&node, &id)["name"], "x");
        assert_eq!(commits(&node, &id).len(), before);
    })
    .await;
}

#[nuclei::test]
async fn less_common_kinds_keep_their_values() {
    edge::compat(async move {
        let cluster = node().await;
        let node = cluster.client(0);

        let id = create(&node, r#"{label: "n", n: 1}"#);
        update(&node, &id, "{n: -3}");
        assert_eq!(row(&node, &id)["n"], -2, "Int pncounter below zero");

        let id = create(&node, r#"{label: "f", f: 1.5}"#);
        update(&node, &id, "{f: 2.25}");
        assert_eq!(row(&node, &id)["f"].as_f64(), Some(3.75), "Float pcounter");

        let id = create(&node, r#"{label: "g", g: 1.5}"#);
        update(&node, &id, "{g: -0.25}");
        assert_eq!(
            row(&node, &id)["g"].as_f64(),
            Some(1.25),
            "Float32 pncounter"
        );

        let id = create(&node, r#"{label: "s", name: "x"}"#);
        update(&node, &id, "{name: null}");
        assert!(row(&node, &id)["name"].is_null(), "LWW set to null");

        let id = create(&node, r#"{label: "d", hits: 4, n: -1}"#);
        node.query(&format!(
            r#"mutation {{ delete_Item(docID: "{id}") {{ _docID }} }}"#
        ))
        .expect("delete_Item");
        let data = node
            .query(&format!(r#"query {{ Item(docID: "{id}") {{ _docID }} }}"#))
            .expect("query deleted");
        assert_eq!(data["Item"].as_array().map(Vec::len), Some(0), "{data}");
    })
    .await;
}

const EXPECTED_CREATE: &[&str] = &[
    "1 _C bafyreiew55i4dr7tjae4bal5gqplhiqcie77x74syfpoyfmlngtkblicyy",
    "1 hits bafyreihdfvyndmv4ap6j7pbcsm3vnlubxs3fosq47q7yri3u4gpfpa7ddy",
    "1 label bafyreib2x4dcf33ae64ytij7h6vmt6jfq667yjf4u6tocj5yyxluh32qvq",
    "1 name bafyreifojwsz6ar65fsa6pk2ba33qkkx6cbpmj5vepkrcowazth3zk2enu",
];
// A counter update carries a random nonce, so its block CID and the composite
// block over it differ on every run (as in Go); their position is pinned, the
// LWW block's CID is exact.
const EXPECTED_UPDATE: &[&str] = &[
    "1 _C bafyreiew55i4dr7tjae4bal5gqplhiqcie77x74syfpoyfmlngtkblicyy",
    "1 hits bafyreihdfvyndmv4ap6j7pbcsm3vnlubxs3fosq47q7yri3u4gpfpa7ddy",
    "1 label bafyreib2x4dcf33ae64ytij7h6vmt6jfq667yjf4u6tocj5yyxluh32qvq",
    "1 name bafyreifojwsz6ar65fsa6pk2ba33qkkx6cbpmj5vepkrcowazth3zk2enu",
    "2 _C <random>",
    "2 hits <random>",
    "2 name bafyreihqnfxeymej3qalhhivnnudgj5w7hxmi3qp56hdzf7wmfgmfj25r4",
];
const ZERO_INCREMENT_BLOCKS: usize = 2;
