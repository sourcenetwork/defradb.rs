//! A browser peer and a native node replicating through a relay.
//!
//! `tools/browser-p2p-e2e.sh` starts a node that hosts an iroh relay and runs
//! local document ACP, then builds this test with the `relay-e2e` feature and
//! `DEFRA_E2E_API` and `DEFRA_E2E_RELAY` set. Missing either fails the build,
//! so there is no way for these tests to run without a node behind them.
//!
//! Neither side has a direct path to the other, so every byte crosses the
//! relay. Blocks a browser's public API cannot build, such as forged
//! signatures, are covered against a native peer in the integration suite.

#![cfg(target_arch = "wasm32")]

mod browser;
mod governed;
mod key;
mod node;
mod protected;
mod protected_update;
mod signed;

use wasm_bindgen_test::*;

use browser::Browser;

wasm_bindgen_test_configure!(run_in_browser);

const SCHEMA: &str = "type Note { text: String }";

#[wasm_bindgen_test]
async fn a_browser_and_a_node_replicate_through_the_hosted_relay() {
    node::add_schema(SCHEMA, None).await;
    node::graphql(
        r#"mutation { create_Note(input: {text: "from-node"}) { _docID } }"#,
        None,
    )
    .await;

    let mut browser = Browser::start("p2p_relay_e2e", None, SCHEMA).await;
    node::replicate_to(&browser.address, &["Note"]).await;
    node::wait_for("the node's document to reach the browser", || async {
        has_note(&browser.query("{ Note { text } }").await, "from-node")
    })
    .await;

    browser.replicate_to_node(&["Note"]).await;
    browser
        .mutate(r#"mutation { create_Note(input: {text: "from-browser"}) { _docID } }"#)
        .await;
    node::wait_for("the browser's document to reach the node", || async {
        has_note(
            &node::graphql("{ Note { text } }", None).await,
            "from-browser",
        )
    })
    .await;

    browser.client.close().await.unwrap();
}

/// A relay started `--governed` does not merge what its rule rejects from a
/// browser, and a browser created with `governance` does not merge what its
/// rule rejects from the relay.
#[wasm_bindgen_test]
async fn governance_holds_across_the_relay_in_both_directions() {
    governed::run().await;
}

fn has_note(data: &serde_json::Value, text: &str) -> bool {
    data["Note"]
        .as_array()
        .is_some_and(|notes| notes.iter().any(|note| note["text"] == text))
}
