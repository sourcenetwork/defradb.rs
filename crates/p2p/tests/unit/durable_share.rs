use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore};

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
#[tokio::test]
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
