use serial_test::serial;

use super::protected::{Outcome, ProtectedNode};
use super::receiver::{user_by_id, wait_for_user};
use integration_test::hostile::blocks::{
    aged, author, genesis, genesis_parent_of, update, user, with_creator, Author, Fragment, Parent,
    Signing,
};

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

#[nuclei::test]
#[serial]
async fn update_to_another_owners_protected_document_is_refused() {
    edge::compat(async move {
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
    })
    .await;
}

#[nuclei::test]
#[serial]
async fn unsigned_update_to_protected_document_is_refused() {
    edge::compat(async move {
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
    })
    .await;
}

/// A replicated genesis registers its verified signer as the owner, so the
/// document is private on the node and a stranger's update is refused.
#[nuclei::test]
#[serial]
async fn update_to_replicated_protected_document_is_refused() {
    edge::compat(async move {
        let owner = author(0x11);
        let stranger = author(0x22);
        let node = ProtectedNode::start(&owner).await;
        let version_id = &node.collection.version_id;

        let created = genesis(&user("Alice", 30), version_id, &owner, Signing::EveryBlock);
        node.peer
            .push(&created, &node.collection.collection_id)
            .await;
        wait_for_user(
            &node.client(),
            Some(&owner.private_key_hex),
            &created.doc_id,
            |row| row["name"] == "Alice",
        )
        .await;
        assert!(
            user_by_id(&node.client(), None, &created.doc_id).is_none(),
            "a replicated protected document is readable without an identity"
        );

        let parent = genesis_parent_of(&created);
        let edited = update(
            &parent,
            &aged(666),
            version_id,
            &stranger,
            Signing::EveryBlock,
        );
        assert_refused(&node, &owner, &created.doc_id, &parent, &edited).await;
    })
    .await;
}
