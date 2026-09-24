use serial_test::serial;

use super::blocks::{author, delete, with_creator};
use super::protected::{Outcome, ProtectedNode};
use super::receiver::user_by_id;

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn delete_by_signer_without_delete_permission_is_refused() {
    let owner = author(0x11);
    let attacker = author(0x22);
    let node = ProtectedNode::start(&owner).await;
    let (target, target_parent) = node.create(&owner, "Alice", 30);
    let (control, control_parent) = node.create(&owner, "Bob", 40);

    let hostile = with_creator(
        &delete(&target_parent, &node.collection.version_id, &attacker),
        &owner.did,
    );
    node.peer
        .push(&hostile, &node.collection.collection_id)
        .await;
    node.rename_as_owner(&owner, &control, &control_parent, "Bob (owner)")
        .await;
    // Bob's rename only shows this peer's pushes arrive; this says the delete
    // itself has been decided.
    let outcome = node.wait_until_judged(&owner, &target, &hostile.root).await;

    let row = user_by_id(&node.client(), Some(&owner.private_key_hex), &target);
    assert!(
        row.as_ref().is_some_and(|row| row["name"] == "Alice"),
        "a peer's delete signed by a non-deleter removed the protected document: {row:?}"
    );
    assert_eq!(outcome, Outcome::Quarantined);
}
