//! Updates a browser makes to a document another identity owns on the node.
//!
//! The document is unregistered in the browser's own ACP, so the browser
//! accepts the edit locally, as any receiving peer would. Only the node, which
//! registered the owner, can refuse it, and it must judge by the signer.
//!
//! Each refusal is checked only after a later write from the same browser has
//! reached the node, so an unchanged document means refused, not undelivered.

use wasm_bindgen_test::*;

use crate::browser::Browser;
use crate::key::Key;
use crate::node;
use crate::protected::Protected;

/// Replicates the node's documents into the browser and back out again.
async fn browser_holding(
    protected: &Protected,
    db_name: &str,
    key: Option<&Key>,
    doc_ids: &[&str],
) -> Browser {
    let collection = protected.collection.as_str();
    let browser = Browser::start(db_name, key, &protected.sdl).await;
    node::replicate_to(&browser.address, &[collection]).await;
    node::wait_for("the protected documents to reach the browser", || async {
        let held = node::doc_ids(
            &browser
                .query(&format!("{{ {collection} {{ _docID }} }}"))
                .await[collection],
        );
        doc_ids.iter().all(|id| held.iter().any(|held| held == id))
    })
    .await;
    browser.replicate_to_node(&[collection]).await;
    browser
}

#[wasm_bindgen_test]
async fn a_browser_cannot_update_a_protected_document_it_does_not_own() {
    let protected = Protected::start("GuardedNote").await;
    let target = protected.create("target", 30).await;
    let granted = protected.create("granted", 30).await;
    let writer = Key::generate();
    protected.grant_writer(&granted, &writer.did).await;

    let mut browser = browser_holding(
        &protected,
        "p2p_relay_protected",
        Some(&writer),
        &[&target, &granted],
    )
    .await;

    browser.mutate(&protected.set_age(&target, 666)).await;
    browser.mutate(&protected.set_age(&granted, 31)).await;
    node::wait_for("the granted update to reach the node", || async {
        protected.age_on_node(&granted).await == Some(31)
    })
    .await;

    assert_eq!(
        protected.age_on_node(&target).await,
        Some(30),
        "a browser without update permission changed the owner's document"
    );
    browser.client.stop_p2p().await.unwrap();
}

#[wasm_bindgen_test]
async fn an_unsigned_browser_update_to_a_protected_document_is_refused() {
    let protected = Protected::start("UnsignedGuardedNote").await;
    let target = protected.create("target", 30).await;

    let mut browser = browser_holding(&protected, "p2p_relay_unsigned", None, &[&target]).await;

    browser.mutate(&protected.set_age(&target, 666)).await;
    // A new document is not an update, so the node registers no owner for it
    // and merges it: the delivery control an unsigned browser can produce.
    let collection = &protected.collection;
    let created = browser
        .mutate(&format!(
            r#"mutation {{ create_{collection}(input: {{name: "control", age: 1}}) {{ _docID }} }}"#
        ))
        .await;
    let control = node::created_doc_id(&created);
    node::wait_for(
        "the unsigned browser's new document to reach the node",
        || async { protected.exists_on_node(&control).await },
    )
    .await;

    assert_eq!(
        protected.age_on_node(&target).await,
        Some(30),
        "an unsigned update changed the owner's document"
    );
    browser.client.stop_p2p().await.unwrap();
}
