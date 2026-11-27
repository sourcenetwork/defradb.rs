use super::*;
use storage::{backends::RegolithStore, stores::Peerstore};

const PEER: &str = "peer";

fn store() -> Peerstore<RegolithStore> {
    Peerstore::new(std::sync::Arc::new(RegolithStore::in_memory().unwrap()))
}

fn retry() -> RetryOptions {
    corekv::engine_retry_options(5)
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn a_narrowed_record_never_gains_a_collection() {
    let stored = p2p::ReplicatorInfo::from_raw(PEER.into(), ids(&["users"]), Vec::new());

    let narrowed = narrowed(stored, &ids(&["users", "articles", "chunks"]));

    assert_eq!(narrowed.collections, ids(&["users"]));
}

#[nuclei::test]
async fn a_peer_with_no_stored_record_gets_none() {
    let store = store();

    narrow(&store, retry(), PEER, &ids(&["users"]))
        .await
        .unwrap();

    assert!(store.get_replicator(PEER).await.unwrap().is_none());
}

#[nuclei::test]
async fn an_unreadable_record_is_left_as_it_is() {
    let store = store();
    store
        .create_replicator(PEER, b"not a record")
        .await
        .unwrap();

    assert!(narrow(&store, retry(), PEER, &[]).await.is_err());

    let bytes = store.get_replicator(PEER).await.unwrap().unwrap();
    assert_eq!(bytes.as_ref(), b"not a record");
}

#[nuclei::test]
async fn a_narrowed_record_takes_a_new_incarnation() {
    let store = store();
    let info = p2p::ReplicatorInfo::from_raw(PEER.into(), ids(&["users", "notes"]), Vec::new());
    store
        .create_replicator(PEER, &info.to_bytes().unwrap())
        .await
        .unwrap();
    let before = store.get_replicator_incarnation(PEER).await.unwrap();

    narrow(&store, retry(), PEER, &ids(&["notes"]))
        .await
        .unwrap();

    let stored = store.get_replicator(PEER).await.unwrap().unwrap();
    let stored = p2p::ReplicatorInfo::from_bytes(&stored).unwrap();
    assert_eq!(stored.collections, ids(&["notes"]));
    assert!(store.get_replicator_incarnation(PEER).await.unwrap() > before);
}
