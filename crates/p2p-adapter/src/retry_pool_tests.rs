//! The failure recorder's writes and a reconnect's activation run on the pool
//! the peerstore is placed on: none of them is made off it, which the store
//! counts.

use std::sync::Arc;

use p2p::transport::PeerId;
use storage::corekv::Store;
use storage::stores::Peerstore;

use crate::retry::{activate_retry_peer, spawn_failure_recorder};

#[test]
fn the_recorder_and_peer_activation_write_on_the_pool() {
    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let stats = store.transaction_stats_handle().unwrap();
    let peerstore = Peerstore::new(Arc::clone(&store)).placed_on(Some(&pool));
    let replicator = p2p::ReplicatorInfo::from_raw(
        "peer-a".to_string(),
        vec!["collection-a".to_string()],
        Vec::new(),
    )
    .to_bytes()
    .unwrap();
    workers::block_on(peerstore.run(move |peerstore| async move {
        peerstore.create_replicator("peer-a", &replicator).await
    }))
    .unwrap()
    .unwrap();
    let before = stats.snapshot().off_pool_calls;

    let (mut failures, received) = futures::channel::mpsc::channel(1);
    let recorder = spawn_failure_recorder(peerstore.clone(), received);
    let (registered, generation) = futures::channel::oneshot::channel();
    failures
        .try_send(p2p::sync::PushFailure {
            peer_id: "peer-a".to_string(),
            doc_id: "doc-a".to_string(),
            collection_id: "collection-a".to_string(),
            cid: "head-a".to_string(),
            head_priority: 7,
            create_retry: false,
            initial_registration: false,
            acknowledged: false,
            marker_generation: None,
            durable_tx: Some(registered),
            retry_after: None,
            admission_only: false,
        })
        .unwrap();
    assert!(
        workers::block_on(generation).unwrap().unwrap().is_some(),
        "the head was registered"
    );
    drop(failures);
    workers::block_on(recorder).unwrap().unwrap();

    workers::block_on(activate_retry_peer(
        &peerstore,
        &PeerId::new("peer-a".to_string()),
    ))
    .unwrap();

    assert_eq!(
        stats.snapshot().off_pool_calls,
        before,
        "a sweep write reached the store off the pool"
    );
    pool.shutdown_blocking().unwrap();
}
