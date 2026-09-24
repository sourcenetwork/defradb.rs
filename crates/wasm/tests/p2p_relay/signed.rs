//! A browser's signature is what the node keeps as the author of its blocks.

use serde_json::Value;
use wasm_bindgen_test::*;

use crate::browser::Browser;
use crate::key::Key;
use crate::node;

const SCHEMA: &str = "type SignedNote { text: String }";

#[wasm_bindgen_test]
async fn a_signed_browser_documents_signature_survives_the_hop() {
    node::add_schema(SCHEMA, None).await;
    node::subscribe(&["SignedNote"]).await;
    let author = Key::generate();
    let mut browser = Browser::start("p2p_relay_signed", Some(&author), SCHEMA).await;
    browser.replicate_to_node(&["SignedNote"]).await;

    let created = browser
        .mutate(r#"mutation { create_SignedNote(input: {text: "signed"}) { _docID } }"#)
        .await;
    let doc_id = node::created_doc_id(&created);
    let local = browser
        .query(&format!(
            r#"{{ _commits(docID: "{doc_id}") {{ fieldName signature {{ identity type }} }} }}"#
        ))
        .await;
    let local_composite = local["_commits"]
        .as_array()
        .and_then(|commits| commits.iter().find(|commit| is_composite(commit)))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        local_composite["signature"]["identity"], author.public_key_hex,
        "the browser must sign its own composite before it replicates: {local}"
    );
    node::wait_for("the signed document to reach the node", || async {
        node::doc_ids(&node::graphql("{ SignedNote { _docID } }", None).await["SignedNote"])
            .contains(&doc_id)
    })
    .await;

    let commits = node::graphql(
        &format!(
            r#"{{ _commits(docID: "{doc_id}") {{ fieldName signature {{ identity type }} }} }}"#
        ),
        None,
    )
    .await;
    let commits = commits["_commits"].as_array().expect("commits");
    let composite = commits
        .iter()
        .find(|commit| is_composite(commit))
        .unwrap_or_else(|| panic!("no composite commit among {commits:?}"));
    assert_eq!(
        composite["signature"]["identity"], author.public_key_hex,
        "the node must hold the browser's signature on the composite: {composite}"
    );
    assert_eq!(composite["signature"]["type"], "ES256K", "{composite}");
    for commit in commits {
        if let Some(signer) = commit["signature"]["identity"].as_str() {
            assert_eq!(
                signer, author.public_key_hex,
                "a block the browser authored names another signer: {commit}"
            );
        }
    }

    browser.client.stop_p2p().await.unwrap();
}

fn is_composite(commit: &Value) -> bool {
    matches!(commit["fieldName"].as_str(), None | Some("_C"))
}
