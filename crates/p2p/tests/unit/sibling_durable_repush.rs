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
#[nuclei::test]
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

/// The resync sweep lets a durable root into a full map only when its block
/// links the in-memory head of its scope, which it then replaces. A sibling
/// of that head, newer or not, waits for an entry to free.
#[nuclei::test]
async fn a_restored_root_fits_a_full_map_only_by_linking_its_scope_head() {
    let manager = test_manager_with_config(SyncConfig {
        max_pending_dags: 1,
        ..Default::default()
    });
    let (base, _) = head_over("base", 1, vec![]);
    let (current, _) = head_over("current", 2, vec![base]);
    let (child, _) = head_over("child", 3, vec![current]);
    let (sibling, _) = head_over("sibling", 3, vec![base]);

    let mut entry = pending_dag_from("doc", Some("hub"), Instant::now());
    entry.head_priority = Some(2);
    assert!(manager.insert_pending_dag(current, entry));

    let record = PersistedPendingDag {
        doc_id: "doc".to_string(),
        collection_id: "collection".to_string(),
        head_priority: Some(3),
        creator: "creator".to_string(),
        source_peer: Some("hub".to_string()),
        alternate_providers: Vec::new(),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
    };
    assert!(manager.restored_root_fits(child, &[current], &record));
    assert!(!manager.restored_root_fits(sibling, &[base], &record));
}
