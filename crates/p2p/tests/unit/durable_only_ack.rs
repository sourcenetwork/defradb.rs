use super::*;
use crate::sync::pending_store::PendingDagStore;
use regolith::sync::Event;
use storage::corekv::{OnCommit, Store, Writer};

/// A head the full map has no entry for is acknowledged on the record its
/// store committed, before the view has learned of that record. The view
/// learns of a commit in commit-receipt order, so a hook of an earlier
/// commit on the same store (a merge's, on the database's store) holds it
/// back past the reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_durable_only_head_is_acknowledged_before_the_view_learns_its_record() {
    let manager = test_manager_with_config(SyncConfig {
        max_pending_dags: 1,
        ..Default::default()
    });
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    manager
        .install_pending_dag_store(Arc::new(PendingDagStore::new(Arc::clone(&store))))
        .await;
    let (leaf, _) = lww_leaf("a");
    let (resident, resident_bytes) = composite_node("a", leaf, 1);
    manager
        .process_pushlog(
            &broadcast("a", resident, resident_bytes),
            Some("peer-1"),
            false,
            None,
        )
        .await
        .expect("the first head takes the map's one entry");

    let entered = Arc::new(Event::new());
    let release = Arc::new(Event::new());
    let holder = {
        let (store, entered, release) = (
            Arc::clone(&store),
            Arc::clone(&entered),
            Arc::clone(&release),
        );
        n0_future::task::spawn(async move {
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

    let (leaf, _) = lww_leaf("b");
    let (durable_only, durable_only_bytes) = composite_node("b", leaf, 1);
    let outcome = manager
        .process_pushlog(
            &broadcast("b", durable_only, durable_only_bytes),
            Some("peer-2"),
            false,
            None,
        )
        .await;
    let seen_before_release = manager.persisted.contains(&durable_only);
    release.set();
    holder.await.unwrap();

    assert!(
        !seen_before_release,
        "the held hook must keep the view from learning of the record"
    );
    outcome.expect("a head on its committed durable record alone is acknowledged");
    assert_eq!(manager.pending_dag_cids(), vec![resident]);
    assert!(manager.persisted.contains(&durable_only));
}
