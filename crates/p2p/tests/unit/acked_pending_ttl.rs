use super::*;
use crate::sync::manager::pending::PENDING_DAG_TTL;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore};

/// A success reply ends the sender's retry marker, so from then on the
/// receiver's pending entry is the only owner of the head. An acknowledged
/// head stays pending past the TTL, stays due on the clock, and the lease of
/// a fetch already running for it stays current.
#[tokio::test]
async fn an_acknowledged_head_outlives_the_pending_ttl() {
    let manager = test_manager();
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    manager
        .install_pending_dag_store(pending_store.clone())
        .await;
    let (leaf, _) = lww_leaf("name");
    let (head, head_bytes) = composite_node("name", leaf, 1);
    manager
        .process_pushlog(
            &broadcast("doc", head, head_bytes),
            Some("peer-1"),
            false,
            None,
        )
        .await
        .expect("the head is acknowledged");
    assert!(manager.is_pending_dag_owned(&head));
    let running_fetch = manager.pending_dag_lease(head);

    manager.pending_dags.update(|pending| {
        pending.get_mut(&head).unwrap().inserted_at =
            Instant::now() - PENDING_DAG_TTL - Duration::from_secs(1);
    });

    let due: Vec<Cid> = manager
        .claim_due_pending_dag_retries(n0_future::time::Instant::now())
        .into_iter()
        .map(|(root, _)| root)
        .collect();
    assert_eq!(due, vec![head], "the acknowledged head is still due");
    assert!(
        running_fetch.is_current(),
        "the TTL must not cancel a fetch of an acknowledged head"
    );
    assert_eq!(manager.diagnostics().snapshot().pending_dag_expired, 0);
    assert_eq!(pending_store.load_all().await.unwrap().len(), 1);
}
