use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore};
use regolith::sync::Event;
use storage::corekv::{OnCommit, Store, Writer};

fn head(field: &str) -> (Cid, Vec<u8>) {
    let (leaf, _) = lww_leaf(field);
    composite_node(field, leaf, 1)
}

async fn offer(
    manager: &SyncManager<DefraBlockstore<RegolithStore>>,
    field: &str,
    peer: &str,
) -> (Cid, crate::error::Result<()>) {
    let (root, bytes) = head(field);
    let outcome = manager
        .process_pushlog(&broadcast(field, root, bytes), Some(peer), false, None)
        .await;
    (root, outcome)
}

/// A peer at its in-memory quota still has its heads acknowledged, each on a
/// durable record alone, until it holds its share of the durable records; the
/// next head is nacked so the sender keeps its marker, while another peer is
/// still admitted. Once one of the peer's roots merges, it is admitted again.
#[nuclei::test]
async fn a_peer_past_its_durable_share_is_nacked_while_others_are_admitted() {
    let manager = test_manager_with_config(SyncConfig {
        max_pending_dags: 4,
        ..Default::default()
    });
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    manager
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(manager.max_pending_dags_per_peer(), 1);
    assert_eq!(manager.max_persisted_pending_per_peer(), 4);

    let mut held = Vec::new();
    for field in ["a", "b", "c", "d"] {
        let (root, outcome) = offer(&manager, field, "peer-1").await;
        outcome
            .unwrap_or_else(|error| panic!("head {field} within the share was refused: {error}"));
        held.push(root);
    }
    assert_eq!(manager.pending_dag_cids(), vec![held[0]]);
    assert_eq!(manager.persisted_pending_count(), 4);

    let (past_share, refused) = offer(&manager, "e", "peer-1").await;
    assert!(
        matches!(refused, Err(Error::PendingDagCapacity { max: 4 })),
        "a head past the peer's durable share must be nacked: {refused:?}"
    );

    let (other, outcome) = offer(&manager, "f", "peer-2").await;
    outcome.expect("another peer is admitted");
    let mut resident = manager.pending_dag_cids();
    resident.sort_unstable_by_key(Cid::to_string);
    let mut expected = vec![held[0], other];
    expected.sort_unstable_by_key(Cid::to_string);
    assert_eq!(resident, expected);
    assert_eq!(pending_store.load_all().await.unwrap().len(), 5);

    manager.mark_as_merged(&held[1]).await.unwrap();
    let (_, outcome) = offer(&manager, "e", "peer-1").await;
    outcome.expect("the peer is within its share again");
    assert!(manager.persisted.contains(&past_share));
}

/// One peer's roots that never resolve cannot fill the durable store, even
/// while the view of the durable records learns of none of them: the store
/// counts the peer's records in each registration's own transaction, so the
/// peer is nacked past its share and another peer is still admitted. The
/// view learns of a commit in commit-receipt order, and a hook of an earlier
/// commit on the same store holds it back here, as one on the database's
/// store holds it while a merge's hook runs.
#[nuclei::test(threads = 2)]
async fn poison_peer_cannot_fill_durable_cap() {
    let manager = test_manager_with_config(SyncConfig {
        max_pending_dags: 4,
        ..Default::default()
    });
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let pending_store = Arc::new(PendingDagStore::new(Arc::clone(&store)));
    manager
        .install_pending_dag_store(pending_store.clone())
        .await;
    assert_eq!(manager.persisted_pending_capacity(), 16);
    assert_eq!(manager.max_persisted_pending_per_peer(), 4);

    let entered = Arc::new(Event::new());
    let release = Arc::new(Event::new());
    let holder = {
        let (store, entered, release) = (
            Arc::clone(&store),
            Arc::clone(&entered),
            Arc::clone(&release),
        );
        crate::rt::spawn(async move {
            let mut txn = store.new_txn(false).await.unwrap();
            txn.set(b"held", b"v").await.unwrap();
            txn.on_commit(OnCommit::OrderedAsync(Box::new(move || {
                Box::pin(async move {
                    entered.set();
                    release.wait().await;
                    Ok(())
                })
            })))
            .unwrap();
            txn.commit().await.unwrap();
        })
    };
    entered.wait().await;

    let mut acknowledged = 0;
    for index in 0..16 {
        let (_, outcome) = offer(&manager, &format!("poison-{index}"), "poison").await;
        match outcome {
            Ok(()) => acknowledged += 1,
            Err(Error::PendingDagCapacity { max: 4 }) => {}
            Err(error) => panic!("a head past the share is nacked at it: {error}"),
        }
    }
    let (other, admitted) = offer(&manager, "other", "peer-2").await;
    let view_lagged = manager.persisted_pending_count() == 0;
    release.set();
    holder.join().await.unwrap();

    assert!(
        view_lagged,
        "the held hook must keep the view from learning of the records"
    );
    assert_eq!(
        (
            acknowledged,
            admitted.as_ref().err().map(ToString::to_string)
        ),
        (4, None),
        "the poison peer holds its share and the other peer is admitted"
    );
    let records = pending_store.load_all().await.unwrap();
    assert_eq!(records.len(), 5);
    assert_eq!(
        records
            .iter()
            .filter(|(_, record)| record.source_peer.as_deref() == Some("poison"))
            .count(),
        4
    );
    assert!(records.iter().any(|(root, _)| *root == other));
}
