use serial_test::serial;

use super::blocks::{aged, author, update, Signing};
use super::protected::ProtectedNode;
use super::receiver::wait_for_user;

#[nuclei::test]
#[serial]
async fn update_signed_by_a_granted_writer_merges() {
    edge::compat(async move {
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
    })
    .await;
}
