use serial_test::serial;

use super::blocks::{aged, author, child_of, update, with_creator, Signing};
use super::peer::HostilePeer;
use super::protected::{Outcome, ProtectedNode};
use super::receiver::{p2p_addrs, user_by_id};

/// The owner's signature on a child must not launder an ancestor the owner
/// never authorized: each composite in the DAG is judged by its own signer.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn attacker_ancestor_under_owner_update_is_refused() {
    let owner = author(0x11);
    let attacker = author(0x22);
    let node = ProtectedNode::start(&owner).await;
    let (doc_id, parent) = node.create(&owner, "Alice", 30);
    let version_id = &node.collection.version_id;

    let ancestor = with_creator(
        &update(
            &parent,
            &aged(666),
            version_id,
            &attacker,
            Signing::EveryBlock,
        ),
        &owner.did,
    );
    let child = update(
        &child_of(&parent, &ancestor),
        &aged(667),
        version_id,
        &owner,
        Signing::EveryBlock,
    );
    node.peer
        .push(&ancestor, &node.collection.collection_id)
        .await;
    node.peer.push(&child, &node.collection.collection_id).await;

    // Until the refused child resolves, it is this peer's current head for
    // the document and covers the lower-priority control, so another peer sends it.
    let other = HostilePeer::dial(&p2p_addrs(&node.cluster, 0)).await;
    node.rename_as_owner_via(&other, &owner, &doc_id, &parent, "Alice (owner)")
        .await;
    let ancestor_outcome = node
        .wait_until_judged(&owner, &doc_id, &ancestor.root)
        .await;
    let child_outcome = node.wait_until_judged(&owner, &doc_id, &child.root).await;

    let row =
        user_by_id(&node.client(), Some(&owner.private_key_hex), &doc_id).expect("owner reads");
    assert_eq!(
        row["age"], 30,
        "an owner-signed child carried an attacker-signed ancestor into the document: {row}"
    );
    assert_eq!(ancestor_outcome, Outcome::Quarantined);
    assert_eq!(child_outcome, Outcome::Quarantined);
}
