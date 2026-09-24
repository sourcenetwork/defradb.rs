use serial_test::serial;

use super::blocks::{aged, author, genesis, genesis_parent_of, update, user, Signing};
use super::protected::ProtectedNode;
use super::receiver::wait_for_user;

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn update_signed_by_a_granted_writer_merges() {
    let owner = author(0x11);
    let writer = author(0x22);
    let node = ProtectedNode::start(&owner).await;
    let (doc_id, parent) = node.create(&owner, "Alice", 30);

    node.client()
        .acp_relationship_add(
            "User",
            &doc_id,
            "writer",
            &writer.did,
            &owner.private_key_hex,
        )
        .expect("owner grants writer");

    let granted = update(
        &parent,
        &aged(31),
        &node.collection.version_id,
        &writer,
        Signing::EveryBlock,
    );
    node.peer
        .push(&granted, &node.collection.collection_id)
        .await;
    wait_for_user(
        &node.client(),
        Some(&owner.private_key_hex),
        &doc_id,
        |row| row["age"] == 31,
    )
    .await;
}

/// Under local ACP a replicated document is never registered on the receiver,
/// so it is public there, as in Go.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn update_to_unregistered_protected_document_merges() {
    let owner = author(0x11);
    let stranger = author(0x22);
    let node = ProtectedNode::start(&owner).await;
    let version_id = &node.collection.version_id;

    let created = genesis(&user("Carol", 50), version_id, &owner, Signing::EveryBlock);
    node.peer
        .push(&created, &node.collection.collection_id)
        .await;
    wait_for_user(&node.client(), None, &created.doc_id, |row| {
        row["name"] == "Carol"
    })
    .await;

    let edited = update(
        &genesis_parent_of(&created),
        &aged(51),
        version_id,
        &stranger,
        Signing::EveryBlock,
    );
    node.peer
        .push(&edited, &node.collection.collection_id)
        .await;
    wait_for_user(&node.client(), None, &created.doc_id, |row| {
        row["age"] == 51
    })
    .await;
}
