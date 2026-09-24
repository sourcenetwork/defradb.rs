use serial_test::serial;

use super::blocks::{author, genesis, named, user, with_doc_id, Signing};
use super::peer::HostilePeer;
use super::receiver::{start_public, user_by_id, users, wait_for_user};

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn document_id_not_matching_genesis_is_refused() {
    let (cluster, addr, collection) = start_public().await;
    let node = cluster.client(0);
    let signer = author(0x11);
    let peer = HostilePeer::dial(&addr).await;

    // The claimed document never sets `age`, so the mislabeled commit leaking
    // into it shows as a non-null age whichever `name` wins the LWW tie.
    let claimed = genesis(
        &named("Claimed"),
        &collection.version_id,
        &signer,
        Signing::CompositeOnly,
    );
    let genuine = genesis(
        &user("Mislabeled", 7),
        &collection.version_id,
        &signer,
        Signing::CompositeOnly,
    );
    let mislabeled = with_doc_id(&genuine, &claimed.doc_id);
    peer.push(&mislabeled, &collection.collection_id).await;

    peer.push(&claimed, &collection.collection_id).await;
    wait_for_user(&node, None, &claimed.doc_id, |row| row["name"] == "Claimed").await;

    // The envelope's document ID is never trusted: identity is derived from
    // the genesis CID, so the mislabeled commit lands under its own ID.
    wait_for_user(&node, None, &genuine.doc_id, |row| {
        row["name"] == "Mislabeled" && row["age"] == 7
    })
    .await;

    let claimed_row = user_by_id(&node, None, &claimed.doc_id).expect("claimed document");
    assert!(
        claimed_row["name"] == "Claimed" && claimed_row["age"].is_null(),
        "another document's genesis was merged under the claimed ID: {claimed_row}"
    );
    assert_eq!(
        users(&node, None).len(),
        2,
        "each genesis must yield exactly one document"
    );
}
