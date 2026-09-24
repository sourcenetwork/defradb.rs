use serial_test::serial;

use super::blocks::{author, genesis, tampered_field, user, Signing};
use super::peer::HostilePeer;
use super::receiver::{start_public, user_by_id, users, wait_for_user};

#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn block_not_matching_its_cid_is_refused() {
    let (cluster, addr, collection) = start_public().await;
    let node = cluster.client(0);
    let signer = author(0x11);
    let peer = HostilePeer::dial(&addr).await;

    let genuine = genesis(
        &user("Tampered", 1864),
        &collection.version_id,
        &signer,
        Signing::CompositeOnly,
    );
    let (tampered, advertised) = tampered_field(&genuine, "age", 1865);
    let replies = peer.push(&tampered, &collection.collection_id).await;
    let (_, reply) = replies
        .iter()
        .find(|(cid, _)| *cid == advertised)
        .expect("the tampered block was pushed");
    assert!(
        reply.err_message.is_some(),
        "the node acknowledged a block that does not hash to its advertised CID"
    );

    let honest = genesis(
        &user("Honest", 1),
        &collection.version_id,
        &signer,
        Signing::CompositeOnly,
    );
    let replies = peer.push(&honest, &collection.collection_id).await;
    assert!(
        replies.iter().all(|(_, reply)| reply.err_message.is_none()),
        "the honest control was refused: {replies:?}"
    );
    wait_for_user(&node, None, &honest.doc_id, |row| row["name"] == "Honest").await;

    assert!(
        user_by_id(&node, None, &tampered.doc_id).is_none(),
        "a document merged from a block that does not match its CID"
    );
    assert!(
        users(&node, None).iter().all(|row| row["age"] != 1865),
        "the tampered value became readable"
    );
}
