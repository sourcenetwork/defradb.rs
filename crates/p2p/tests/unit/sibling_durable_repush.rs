//! The two admission rules that meet in `assert_pushlog_left_durable_state`:
//! a head retires its scope's current head only when it links it, and a
//! durable root with no in-memory entry still backs the ack of its re-push.

use super::*;
use crate::sync::pending_store::{PendingDagStorage, PendingDagStore};

/// A composite head of document `doc` at `priority` that links `parents`.
fn head_over(field: &str, priority: u64, parents: Vec<Cid>) -> (Cid, Vec<u8>) {
    let (leaf, _) = lww_leaf(field);
    let block = DefraBlock::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            collection_version_id: "schema1".to_string(),
            priority,
            status: 1,
        }),
        parents,
        vec![DAGLink::new(field, leaf)],
    );
    let bytes = block.to_dag_cbor().expect("encode composite block");
    let cid = block.generate_cid().expect("generate composite cid");
    (cid, bytes)
}

fn sorted(mut cids: Vec<Cid>) -> Vec<Cid> {
    cids.sort_unstable_by_key(Cid::to_string);
    cids
}

async fn durable_roots(store: &PendingDagStore<RegolithStore>) -> Vec<Cid> {
    sorted(
        store
            .load_all()
            .await
            .unwrap()
            .into_iter()
            .map(|(root, _)| root)
            .collect(),
    )
}

/// Two heads of one document written concurrently elsewhere reach this node
/// from one sender (a hub forwards both under its own ID). The newer one does
/// not link the older, so both keep their entry and their record. A head
/// that links the newer one retires it, in memory and durably, and leaves the
/// unlinked sibling. That sibling, once its entry is rotated out, is no
/// longer the scope's current head and nothing links it; its re-push is
/// still acknowledged, on its durable record alone, and writes nothing.
#[tokio::test]
async fn siblings_keep_their_records_and_a_rotated_sibling_is_acked_on_repush() {
    let manager = test_manager();
    let pending_store = Arc::new(PendingDagStore::new(Arc::new(
        RegolithStore::in_memory().unwrap(),
    )));
    manager
        .install_pending_dag_store(pending_store.clone())
        .await;

    let (base, _) = head_over("base", 1, vec![]);
    let (left, left_bytes) = head_over("left", 2, vec![base]);
    let (right, right_bytes) = head_over("right", 3, vec![base]);
    let (next, next_bytes) = head_over("next", 4, vec![right]);
    let push = |root, bytes: &Vec<u8>| broadcast("doc", root, bytes.clone());

    for (root, bytes) in [(left, &left_bytes), (right, &right_bytes)] {
        manager
            .process_pushlog(&push(root, bytes), Some("hub"), false, None)
            .await
            .expect("a sibling head is acknowledged");
    }
    assert_eq!(
        sorted(manager.pending_dag_cids()),
        sorted(vec![left, right]),
        "the newer sibling does not link the older, so it retires nothing"
    );
    assert_eq!(
        durable_roots(&pending_store).await,
        sorted(vec![left, right])
    );

    manager
        .process_pushlog(&push(next, &next_bytes), Some("hub"), false, None)
        .await
        .expect("a head linking the current one is acknowledged");
    assert_eq!(
        sorted(manager.pending_dag_cids()),
        sorted(vec![left, next]),
        "only the linked head is retired"
    );
    assert_eq!(
        durable_roots(&pending_store).await,
        sorted(vec![left, next])
    );

    // What a rotation does: the entry goes, the record stays.
    manager.pending_dags.update(|pending| pending.remove(&left));
    assert!(manager.pending_dag_snapshot(&left).is_none());

    for peer in ["hub", "peer-2"] {
        manager
            .process_pushlog(&push(left, &left_bytes), Some(peer), false, None)
            .await
            .unwrap_or_else(|error| {
                panic!("re-push of the sibling from {peer} was refused: {error}")
            });
    }
    assert_eq!(
        durable_roots(&pending_store).await,
        sorted(vec![left, next]),
        "the re-push neither retires nor adds a record"
    );
    assert_eq!(manager.pending_dag_cids(), vec![next]);
}
