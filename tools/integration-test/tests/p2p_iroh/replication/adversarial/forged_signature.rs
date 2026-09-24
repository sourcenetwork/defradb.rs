use serial_test::serial;

use super::blocks::{author, forged_signature, genesis, user, Signing};
use super::peer::HostilePeer;
use super::receiver::{start_public, user_by_id, users, wait_for_user};

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn forged_signature_is_refused() {
    let (cluster, addr, collection) = start_public().await;
    let node = cluster.client(0);
    let signer = author(0x11);
    let impostor = author(0x22);
    let peer = HostilePeer::dial(&addr).await;

    let forged = forged_signature(
        &user("Forged", 2),
        &collection.version_id,
        &signer,
        &impostor,
    );
    peer.push(&forged, &collection.collection_id).await;

    let honest = genesis(
        &user("Honest", 1),
        &collection.version_id,
        &signer,
        Signing::CompositeOnly,
    );
    peer.push(&honest, &collection.collection_id).await;
    wait_for_user(&node, None, &honest.doc_id, |row| row["name"] == "Honest").await;

    assert!(
        user_by_id(&node, None, &forged.doc_id).is_none(),
        "a root whose signature does not cover it was merged"
    );
    assert!(
        users(&node, None).iter().all(|row| row["name"] != "Forged"),
        "the forged commit's values became readable"
    );
}
