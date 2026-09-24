use serial_test::serial;

use super::blocks::{aged, author, update, with_creator, Author, Fragment, Parent, Signing};
use super::protected::{Outcome, ProtectedNode};
use super::receiver::user_by_id;

/// Push `hostile`, then the owner's own rename; once the node has judged the
/// hostile root, check the hostile age never landed.
async fn assert_refused(
    node: &ProtectedNode,
    owner: &Author,
    doc_id: &str,
    parent: &Parent,
    hostile: &Fragment,
) {
    node.peer
        .push(hostile, &node.collection.collection_id)
        .await;
    node.rename_as_owner(owner, doc_id, parent, "Alice (owner)")
        .await;
    let outcome = node.wait_until_judged(owner, doc_id, &hostile.root).await;

    let row =
        user_by_id(&node.client(), Some(&owner.private_key_hex), doc_id).expect("owner reads");
    assert_eq!(
        row["age"], 30,
        "a peer's update without the owner's authority changed the protected document: {row}"
    );
    assert_eq!(outcome, Outcome::Quarantined);
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn update_to_another_owners_protected_document_is_refused() {
    let owner = author(0x11);
    let attacker = author(0x22);
    let node = ProtectedNode::start(&owner).await;
    let (doc_id, parent) = node.create(&owner, "Alice", 30);

    // Signed by the attacker, while the envelope names the owner as creator.
    let hijack = with_creator(
        &update(
            &parent,
            &aged(666),
            &node.collection.version_id,
            &attacker,
            Signing::EveryBlock,
        ),
        &owner.did,
    );
    assert_refused(&node, &owner, &doc_id, &parent, &hijack).await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn unsigned_update_to_protected_document_is_refused() {
    let owner = author(0x11);
    let node = ProtectedNode::start(&owner).await;
    let (doc_id, parent) = node.create(&owner, "Alice", 30);

    let unsigned = update(
        &parent,
        &aged(666),
        &node.collection.version_id,
        &owner,
        Signing::Unsigned,
    );
    assert_refused(&node, &owner, &doc_id, &parent, &unsigned).await;
}
