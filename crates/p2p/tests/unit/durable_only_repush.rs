use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore};

/// A durable root can be without an in-memory entry: it gave its entry up to
/// a root waiting past the cap (`PendingDagLease::finish_attempt`), or the
/// node restarted. Its record still owns the head, so a re-push of it, from
/// its sender or another peer, is acknowledged rather than nacked as not
/// durable (which the sender reads as rate limiting and parks the peer).
#[tokio::test]
async fn a_repush_of_a_durable_root_without_an_entry_is_acknowledged() {
    let manager = test_manager();
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    manager
        .install_pending_dag_store(pending_store.clone())
        .await;
    let (leaf, _) = lww_leaf("name");
    let (head, head_bytes) = composite_node("name", leaf, 1);
    let push = broadcast("doc", head, head_bytes);
    manager
        .process_pushlog(&push, Some("peer-1"), false, None)
        .await
        .expect("the head is acknowledged");

    // What a rotation does: the entry goes, the record stays.
    manager.pending_dags.update(|pending| pending.remove(&head));
    assert!(manager.pending_dag_snapshot(&head).is_none());
    assert_eq!(pending_store.load_all().await.unwrap().len(), 1);

    for peer in ["peer-1", "peer-2"] {
        manager
            .process_pushlog(&push, Some(peer), false, None)
            .await
            .unwrap_or_else(|error| panic!("re-push from {peer} was refused: {error}"));
    }
    assert_eq!(pending_store.load_all().await.unwrap().len(), 1);
}
