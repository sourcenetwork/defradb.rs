//! Governance across the relay, in both directions.
//!
//! `tools/browser-p2p-e2e.sh` starts the node with `--governed Ledger
//! --rule-module reject.wasm`, the module below, which rejects every
//! composite with reason "forged". The browser governs `Post` with the same
//! module and the node does not; `Receipt` is governed by neither and is
//! the control that shows a push arrived.

use serde_json::{json, Value};

use crate::browser::Browser;
use crate::node;

/// A guest answering `{"verdict":"reject","reason":"forged"}` at every
/// step. The script writes the same bytes for the node.
pub const REJECT_MODULE_HEX: &str = "0061736d01000000010c0260017f017f60027f7f017f03030200010503010001071a03066d656d6f7279020005616c6c6f630000056a7564676500010a0d0205004180200b05004180080b0b2901004180080b221e000000a267766572646963746672656a65637466726561736f6e66666f72676564";

fn sdl(rule: &str) -> String {
    format!(
        r#"type Ledger @governed(root: "e2e", rule: "{rule}") {{ text: String }}
type Post @governed(root: "e2e", rule: "{rule}") {{ text: String }}
type Receipt {{ text: String }}"#
    )
}

fn texts(data: &Value, collection: &str) -> Vec<String> {
    data[collection]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["text"].as_str().map(String::from))
        .collect()
}

/// Long enough for a push sent before the control to have been judged.
async fn settle() {
    gloo_timers::future::TimeoutFuture::new(2_000).await;
}

pub async fn run() {
    let module = hex::decode(REJECT_MODULE_HEX).unwrap();
    let rule = db::merge::governance::rule::module_cid(&module).to_string();
    let sdl = sdl(&rule);
    node::add_schema(&sdl, None).await;
    node::subscribe(&["Ledger", "Post", "Receipt"]).await;

    // Browser to node: the browser governs nothing, so its Ledger write
    // commits there and is pushed; the node's rule rejects it.
    let mut plain = Browser::start("governed_e2e_plain", None, &sdl).await;
    plain.replicate_to_node(&["Ledger", "Receipt"]).await;
    plain
        .mutate(r#"mutation { create_Ledger(input: {text: "forged"}) { _docID } }"#)
        .await;
    plain
        .mutate(r#"mutation { create_Receipt(input: {text: "from-browser"}) { _docID } }"#)
        .await;
    node::wait_for("the browser's receipt to reach the node", || async {
        texts(
            &node::graphql("{ Receipt { text } }", None).await,
            "Receipt",
        )
        .contains(&"from-browser".to_string())
    })
    .await;
    settle().await;
    assert!(
        texts(&node::graphql("{ Ledger { text } }", None).await, "Ledger").is_empty(),
        "the node merged a ledger entry its rule rejects"
    );
    // The node judges its own HTTP writes by the same rule.
    let refused = node::post_json(
        "/api/v0/graphql",
        json!({ "query": r#"mutation { create_Ledger(input: {text: "forged"}) { _docID } }"# }),
        None,
    )
    .await;
    assert!(
        refused["errors"].to_string().contains("forged"),
        "the node committed a local write its rule rejects: {refused}"
    );

    // Node to browser: the node governs no Post, so its Post write commits
    // there and is pushed; the browser's rule rejects it.
    let mut governed = Browser::start_with(
        json!({
            "db_name": "governed_e2e_rules",
            "governance": { "collections": ["Post"], "rule_modules": [module] },
        }),
        &sdl,
    )
    .await;
    let status = governed.governance().await;
    assert_eq!(status["rules"][0]["held"], true, "{status}");
    node::graphql(
        r#"mutation { create_Post(input: {text: "forged"}) { _docID } }"#,
        None,
    )
    .await;
    node::graphql(
        r#"mutation { create_Receipt(input: {text: "from-node"}) { _docID } }"#,
        None,
    )
    .await;
    node::replicate_to(&governed.address, &["Post", "Receipt"]).await;
    node::wait_for("the node's receipt to reach the browser", || async {
        texts(&governed.query("{ Receipt { text } }").await, "Receipt")
            .contains(&"from-node".to_string())
    })
    .await;
    settle().await;
    assert!(
        texts(&governed.query("{ Post { text } }").await, "Post").is_empty(),
        "the browser merged a post its rule rejects"
    );

    plain.client.close().await.unwrap();
    governed.client.close().await.unwrap();
}
