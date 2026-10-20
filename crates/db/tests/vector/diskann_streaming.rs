//! DiskANN once trained: inserts land in a pending set rather than the graph,
//! and a merge folds pending inserts and tombstones into the graph in the
//! paper's three phases (FreshDiskANN's StreamingMerge).

use crate::diskann_engine::{all, built, corpus, count, params, recall, CORPUS, DIMENSIONS, K};
use db::index::vector::engine::ann::VectorIndexEngine;
use db::index::vector::engine::diskann::codec;
use db::index::vector::index::VectorIndex;
use db::index::vector::store::{KvNodeStore, NodeId, VectorNodeStore};
use document::NormalValue;
use schema::{
    DistanceMetric, IndexDescription, IndexedFieldDescription, VectorAlgorithm,
    VectorIndexDescription,
};
use storage::corekv::Store;
use storage::index::CollectionIndex;
use storage::RegolithStore;

pub fn fresh(seed: u64) -> Vec<f32> {
    crate::support::Corpus::new(seed).vector(DIMENSIONS)
}

#[test]
fn an_insert_after_training_only_joins_the_pending_set() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let records = count(&index, codec::GRAPH).await;
        let state = index.state().await.unwrap().unwrap();

        let id = NodeId(CORPUS as u64 + 1);
        let vector = fresh(0x9E4D);
        index.insert(id, &vector).await.unwrap();

        assert_eq!(count(&index, codec::GRAPH).await, records, "no graph write");
        assert_eq!(count(&index, codec::PENDING).await, 1);
        assert_eq!(
            index.state().await.unwrap().unwrap(),
            state,
            "an insert leaves the shared state alone"
        );
        let hits = index.search(vector.as_slice(), K, None).await.unwrap();
        assert_eq!(hits[0].id, id, "a pending vector is searchable at once");
    })
    .unwrap()
}

#[test]
fn a_merge_folds_pending_inserts_into_the_graph() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let (base, extra) = vectors.split_at(CORPUS - 150);
        let mut index = built(base).await;
        for (i, vector) in extra.iter().enumerate() {
            index
                .insert(NodeId((base.len() + i) as u64 + 1), vector)
                .await
                .unwrap();
        }
        assert_eq!(count(&index, codec::PENDING).await, extra.len());
        assert!(
            index.should_build().await.unwrap(),
            "pending inserts are due"
        );
        assert!(
            !index.should_build_inline().await.unwrap(),
            "a write never merges inline"
        );

        let before = recall(&index, &vectors, &all(CORPUS), &queries).await;
        index.build().await.unwrap();
        assert_eq!(count(&index, codec::PENDING).await, 0);
        assert_eq!(count(&index, codec::GRAPH).await, CORPUS);
        assert_eq!(index.state().await.unwrap().unwrap().live, CORPUS as u64);
        assert!(!index.should_build().await.unwrap());

        let after = recall(&index, &vectors, &all(CORPUS), &queries).await;
        assert!(after >= 0.9, "recall@{K} after a merge is {after}");
        assert!(
            before >= 0.9,
            "recall@{K} over graph and pending is {before}"
        );
    })
    .unwrap()
}

/// The merge's patch phase is what links a merged node back in: without it a
/// merged node has out-edges but nothing points at it, and no walk reaches it.
#[test]
fn a_merged_node_is_reachable_through_back_edges() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors[..CORPUS - 100]).await;
        let ids: Vec<NodeId> = (CORPUS - 100..CORPUS)
            .map(|i| NodeId(i as u64 + 1))
            .collect();
        for id in &ids {
            index
                .insert(*id, &vectors[id.0 as usize - 1])
                .await
                .unwrap();
        }
        index.build().await.unwrap();

        let m = index.state().await.unwrap().unwrap().m as usize;
        let mut pointed_at = rapidhash::RapidHashSet::default();
        let limit = params().slack_limit();
        index
            .store()
            .iterate_aux(codec::GRAPH, b"", |_, value| {
                let record = codec::decode_record(value, m)?;
                assert!(record.neighbors.len() <= limit);
                pointed_at.extend(record.neighbors.into_iter().map(|(id, _)| id));
                Ok(())
            })
            .await
            .unwrap();
        for id in &ids {
            assert!(pointed_at.contains(id), "{id:?} has no in-edge");
        }
    })
    .unwrap()
}

#[test]
fn a_pending_delete_never_reaches_the_graph() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let id = NodeId(CORPUS as u64 + 1);
        let vector = fresh(0xDE1E);
        index.insert(id, &vector).await.unwrap();
        assert!(index.delete(id).await.unwrap());

        assert_eq!(count(&index, codec::PENDING).await, 0);
        let hits = index.search(vector.as_slice(), K, None).await.unwrap();
        assert!(hits.iter().all(|hit| hit.id != id));

        index.build().await.unwrap();
        assert_eq!(count(&index, codec::GRAPH).await, CORPUS);
        assert_eq!(index.state().await.unwrap().unwrap().live, CORPUS as u64);
    })
    .unwrap()
}

/// A tombstone left for a node that never reached the graph is dropped by
/// consolidation without counting against the live nodes.
#[test]
fn a_tombstone_without_a_record_is_dropped_by_consolidation() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let orphan = NodeId(CORPUS as u64 + 1);
        index.insert(orphan, &fresh(0x0F0F)).await.unwrap();
        index.delete(orphan).await.unwrap();
        let deleted: Vec<NodeId> = (1..=130).map(NodeId).collect();
        for id in &deleted {
            index.delete(*id).await.unwrap();
        }
        assert_eq!(count(&index, codec::DELETED).await, deleted.len() + 1);

        index.build().await.unwrap();
        assert_eq!(count(&index, codec::DELETED).await, 0);
        assert_eq!(count(&index, codec::GRAPH).await, CORPUS - deleted.len());
        assert_eq!(
            index.state().await.unwrap().unwrap().live,
            (CORPUS - deleted.len()) as u64
        );
    })
    .unwrap()
}

#[test]
fn a_merged_delete_is_consolidated_by_the_merge() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        // Past the consolidation threshold: a tenth of the graph.
        let deleted: Vec<NodeId> = (1..=130).map(NodeId).collect();
        for id in &deleted {
            index.delete(*id).await.unwrap();
        }
        assert_eq!(count(&index, codec::DELETED).await, deleted.len());
        assert!(
            index.should_build().await.unwrap(),
            "130 tombstones are due"
        );

        index.build().await.unwrap();
        assert_eq!(count(&index, codec::DELETED).await, 0);
        assert_eq!(count(&index, codec::GRAPH).await, CORPUS - deleted.len());
        assert_eq!(
            index.state().await.unwrap().unwrap().live,
            (CORPUS - deleted.len()) as u64
        );
    })
    .unwrap()
}

/// An update re-inserts under the same id: the new vector is found before and
/// after the merge, the node is counted once, and is returned once.
#[test]
fn a_reinserted_node_is_found_by_its_new_vector_once() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let moved = fresh(0x40FE);
        index.insert(NodeId(7), &moved).await.unwrap();

        let hits = index.search(moved.as_slice(), K, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(7), "pending: found by its new vector");
        assert_eq!(hits.iter().filter(|hit| hit.id == NodeId(7)).count(), 1);

        index.build().await.unwrap();
        let hits = index.search(moved.as_slice(), K, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(7), "merged: found by its new vector");
        assert_eq!(hits.iter().filter(|hit| hit.id == NodeId(7)).count(), 1);
        assert_eq!(index.state().await.unwrap().unwrap().live, CORPUS as u64);
    })
    .unwrap()
}

#[test]
fn a_recreated_node_leaves_the_delete_list() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        index.delete(NodeId(9)).await.unwrap();
        assert_eq!(count(&index, codec::DELETED).await, 1);

        index.insert(NodeId(9), &vectors[8]).await.unwrap();
        assert_eq!(count(&index, codec::DELETED).await, 0);
        let hits = index.search(vectors[8].as_slice(), 1, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(9));

        index.build().await.unwrap();
        let hits = index.search(vectors[8].as_slice(), 1, None).await.unwrap();
        assert_eq!(hits[0].id, NodeId(9));
        assert_eq!(index.state().await.unwrap().unwrap().live, CORPUS as u64);
    })
    .unwrap()
}

#[test]
fn filtered_search_covers_pending_inserts() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        let id = NodeId(CORPUS as u64 + 2);
        let vector = fresh(0xF117);
        index.insert(id, &vector).await.unwrap();

        let even = |n: NodeId| n.0.is_multiple_of(2);
        let hits = index
            .search_where(vector.as_slice(), K, None, &even)
            .await
            .unwrap();
        assert_eq!(hits[0].id, id);
        assert!(hits.iter().all(|hit| even(hit.id)));
    })
    .unwrap()
}

/// Below the threshold, tombstones wait: finding their in-edges is a pass over
/// every record, too much to pay for a handful.
#[test]
fn a_few_tombstones_wait_for_the_threshold() {
    workers::block_on(async {
        let (vectors, _) = corpus();
        let mut index = built(&vectors).await;
        for id in 1..=10 {
            index.delete(NodeId(id)).await.unwrap();
        }
        assert!(!index.should_build().await.unwrap());
        assert!(index.merge_plan().await.unwrap().is_empty());
        for hit in index.search(vectors[0].as_slice(), K, None).await.unwrap() {
            assert!(hit.id.0 > 10, "returned deleted {:?}", hit.id);
        }
    })
    .unwrap()
}

/// A plan taken from one snapshot and applied on another transaction does the
/// same work as a merge on one: what the background sweep relies on.
#[test]
fn a_plan_applies_like_a_build() {
    workers::block_on(async {
        let (vectors, queries) = corpus();
        let (base, extra) = vectors.split_at(CORPUS - 200);
        let mut index = built(base).await;
        for (i, vector) in extra.iter().enumerate() {
            index
                .insert(NodeId((base.len() + i) as u64 + 1), vector)
                .await
                .unwrap();
        }
        for id in 1..=130 {
            index.delete(NodeId(id)).await.unwrap();
        }
        let plan = index.merge_plan().await.unwrap();
        assert_eq!(plan.pending.len(), extra.len());
        assert_eq!(plan.deleted.len(), 130);
        assert!(!plan.is_empty());

        index.merge(plan).await.unwrap();
        assert_eq!(count(&index, codec::PENDING).await, 0);
        assert_eq!(count(&index, codec::DELETED).await, 0);
        let live: rapidhash::RapidHashSet<u64> = (131..=CORPUS as u64).collect();
        let recall = recall(&index, &vectors, &live, &queries).await;
        assert!(
            recall >= 0.9,
            "recall@{K} after a planned merge is {recall}"
        );
        assert!(index.merge_plan().await.unwrap().is_empty());
    })
    .unwrap()
}

// --- Through `VectorIndex` on regolith, where transactions can conflict. ---

const COLLECTION: u32 = 31;
const INDEX_ID: u32 = 3;

pub(crate) fn description() -> IndexDescription {
    IndexDescription {
        name: "by_embedding".to_string(),
        id: INDEX_ID,
        fields: vec![IndexedFieldDescription {
            name: "embedding".to_string(),
            descending: false,
        }],
        unique: false,
        kind: None,
        auto_generated: false,
    }
    .as_vector(VectorIndexDescription {
        algorithm: VectorAlgorithm::DiskAnn,
        metric: DistanceMetric::Cosine,
        dimensions: DIMENSIONS as u32,
        hnsw: None,
        ivfpq: None,
        ivfflat: None,
        ssg: None,
        diskann: Some(schema::DiskAnnParams {
            r: params().r,
            l_build: params().l_build,
            l_search: params().l_search,
            m: params().m,
            ..Default::default()
        }),
    })
}

pub(crate) fn value(vector: &[f32]) -> Vec<NormalValue> {
    vec![NormalValue::Float32Array(vector.to_vec())]
}

/// Every vector saved and committed: training indexed the first
/// `TRAIN_THRESHOLD`, the rest are pending.
async fn saved_store() -> (RegolithStore, VectorIndex) {
    let store = RegolithStore::in_memory().unwrap();
    let index = VectorIndex::try_new(COLLECTION, description()).unwrap();
    let (vectors, _) = corpus();
    let mut write = store.new_txn(false).await.unwrap();
    for (i, vector) in vectors.iter().enumerate() {
        index
            .save(&mut write, i as u64 + 1, &value(vector))
            .await
            .unwrap();
    }
    write.commit().await.unwrap();
    (store, index)
}

/// Plans on a snapshot and merges on a write, the way the sweep does.
pub(crate) async fn merge_due(store: &RegolithStore, index: &VectorIndex) -> bool {
    let mut snapshot = store.new_txn(true).await.unwrap();
    let plan = index.merge_plan(&mut snapshot).await.unwrap();
    snapshot.discard();
    let Some(plan) = plan else {
        return false;
    };
    let mut write = store.new_txn(false).await.unwrap();
    index.merge(&mut write, plan).await.unwrap();
    write.commit().await.unwrap();
    true
}

/// A trained index on regolith with nothing pending.
pub(crate) async fn trained_store() -> (RegolithStore, VectorIndex) {
    let (store, index) = saved_store().await;
    assert!(merge_due(&store, &index).await);
    assert!(
        !merge_due(&store, &index).await,
        "one merge took everything"
    );
    (store, index)
}

/// The point of the pending set: two writers inserting at once no longer
/// both rewrite the same neighbour records and state, so neither aborts.
#[test]
fn concurrent_inserts_after_training_both_commit() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let mut a = store.new_txn(false).await.unwrap();
        let mut b = store.new_txn(false).await.unwrap();
        let (va, vb) = (fresh(0xA), fresh(0xB));
        index.save(&mut a, 90_001, &value(&va)).await.unwrap();
        index.save(&mut b, 90_002, &value(&vb)).await.unwrap();
        a.commit().await.expect("first insert commits");
        b.commit().await.expect("a concurrent insert commits too");

        let mut read = store.new_txn(true).await.unwrap();
        for (id, vector) in [(90_001u64, &va), (90_002, &vb)] {
            let hits = index.search(&mut read, vector, 1, None).await.unwrap();
            assert_eq!(hits[0].id.0, id);
        }
    })
    .unwrap()
}

/// The sweep plans on a snapshot and merges on its own transaction. An insert
/// that commits in between joins the pending set for the next merge rather
/// than aborting the merge.
#[test]
fn a_merge_commits_alongside_a_concurrent_insert() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let mut write = store.new_txn(false).await.unwrap();
        for i in 0..50u64 {
            index
                .save(&mut write, 80_000 + i, &value(&fresh(i)))
                .await
                .unwrap();
        }
        write.commit().await.unwrap();

        let mut snapshot = store.new_txn(true).await.unwrap();
        let plan = index
            .merge_plan(&mut snapshot)
            .await
            .unwrap()
            .expect("pending inserts are due");
        snapshot.discard();
        assert_eq!(plan.pending.len(), 50);

        let mut merge = store.new_txn(false).await.unwrap();
        let mut insert = store.new_txn(false).await.unwrap();
        let late = fresh(0x1A7E);
        index
            .save(&mut insert, 85_000, &value(&late))
            .await
            .unwrap();
        index.merge(&mut merge, plan).await.unwrap();
        insert.commit().await.expect("the insert commits");
        merge.commit().await.expect("the merge commits after it");

        let mut read = store.new_txn(true).await.unwrap();
        let left = index
            .merge_plan(&mut read)
            .await
            .unwrap()
            .expect("the late insert is still pending");
        assert_eq!(left.pending, vec![NodeId(85_000)]);
        let hits = index.search(&mut read, &late, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, 85_000);
        for i in [0u64, 25, 49] {
            let hits = index.search(&mut read, &fresh(i), 1, None).await.unwrap();
            assert_eq!(hits[0].id.0, 80_000 + i, "merged insert {i} is found");
        }
    })
    .unwrap()
}

/// The other order: a merge commits first, under a writer that began before
/// it. The insert read only the trained state and its own node, which no merge
/// writes, so it still commits.
#[test]
fn an_insert_commits_after_a_merge_that_started_later() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let mut write = store.new_txn(false).await.unwrap();
        for i in 0..20u64 {
            index
                .save(&mut write, 70_000 + i, &value(&fresh(100 + i)))
                .await
                .unwrap();
        }
        write.commit().await.unwrap();

        let mut insert = store.new_txn(false).await.unwrap();
        let late = fresh(0x7A7E);
        index
            .save(&mut insert, 75_000, &value(&late))
            .await
            .unwrap();
        assert!(merge_due(&store, &index).await, "the 20 merge");
        insert
            .commit()
            .await
            .expect("the earlier insert still commits");

        let mut read = store.new_txn(true).await.unwrap();
        let left = index
            .merge_plan(&mut read)
            .await
            .unwrap()
            .expect("one left");
        assert_eq!(left.pending, vec![NodeId(75_000)]);
        let hits = index.search(&mut read, &late, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, 75_000);
    })
    .unwrap()
}

/// A delete that commits while a merge is in flight: the merge read the keys
/// the delete wrote, so it must not commit over it. It aborts, the next merge
/// redoes it, and neither deleted node ever comes back.
#[test]
fn a_delete_during_a_merge_aborts_the_merge_not_the_delete() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let (vectors, _) = corpus();
        let mut write = store.new_txn(false).await.unwrap();
        for i in 0..20u64 {
            index
                .save(&mut write, 60_000 + i, &value(&fresh(200 + i)))
                .await
                .unwrap();
        }
        write.commit().await.unwrap();

        let mut snapshot = store.new_txn(true).await.unwrap();
        let plan = index.merge_plan(&mut snapshot).await.unwrap().unwrap();
        snapshot.discard();
        let mut merge = store.new_txn(false).await.unwrap();
        index.merge(&mut merge, plan).await.unwrap();

        let mut delete = store.new_txn(false).await.unwrap();
        index
            .delete(&mut delete, 60_005, &value(&fresh(205)))
            .await
            .unwrap();
        delete.commit().await.expect("the delete commits");
        let err = merge
            .commit()
            .await
            .expect_err("the merge read what the delete wrote");
        assert!(err.to_string().contains("onflict"), "got: {err}");

        let mut read = store.new_txn(true).await.unwrap();
        let hits = index.search(&mut read, &fresh(205), K, None).await.unwrap();
        assert!(hits.iter().all(|hit| hit.id.0 != 60_005));
        read.discard();

        assert!(merge_due(&store, &index).await, "the next merge redoes it");
        let mut read = store.new_txn(true).await.unwrap();
        assert!(index.merge_plan(&mut read).await.unwrap().is_none());
        let hits = index.search(&mut read, &fresh(205), K, None).await.unwrap();
        assert!(
            hits.iter().all(|hit| hit.id.0 != 60_005),
            "still deleted after the redo"
        );
        let hits = index.search(&mut read, &vectors[2], 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, 3, "the rest of the graph is intact");
    })
    .unwrap()
}

/// The reverse order: a delete that began before a merge and commits after
/// it. The merge rewrote the deleted node's record, but the delete never read
/// it, so both commit and the node stays deleted.
#[test]
fn a_delete_spanning_a_merge_commits_after_it() {
    workers::block_on(async {
        let (store, index) = trained_store().await;
        let mut write = store.new_txn(false).await.unwrap();
        for i in 0..20u64 {
            index
                .save(&mut write, 60_000 + i, &value(&fresh(200 + i)))
                .await
                .unwrap();
        }
        write.commit().await.unwrap();

        // The graph node nearest a pending insert takes a back-edge from it.
        let mut snapshot = store.new_txn(true).await.unwrap();
        let target = index
            .search(&mut snapshot, &fresh(205), K, None)
            .await
            .unwrap()
            .into_iter()
            .find(|hit| hit.id.0 < 60_000)
            .unwrap()
            .id
            .0;
        let mut delete = store.new_txn(false).await.unwrap();
        let plan = index.merge_plan(&mut snapshot).await.unwrap().unwrap();
        snapshot.discard();
        let before = graph_record(&store, target).await;
        let mut merge = store.new_txn(false).await.unwrap();
        index.merge(&mut merge, plan).await.unwrap();
        merge.commit().await.expect("the merge commits");
        assert_ne!(
            graph_record(&store, target).await,
            before,
            "the merge rewrote the record the delete targets"
        );

        let (vectors, _) = corpus();
        let vector = &vectors[target as usize - 1];
        index
            .delete(&mut delete, target, &value(vector))
            .await
            .unwrap();
        delete
            .commit()
            .await
            .expect("the delete never read what the merge wrote");

        let mut read = store.new_txn(true).await.unwrap();
        let hits = index.search(&mut read, vector, K, None).await.unwrap();
        assert!(hits.iter().all(|hit| hit.id.0 != target));
    })
    .unwrap()
}

async fn graph_record(store: &RegolithStore, id: u64) -> Option<Vec<u8>> {
    let mut txn = store.new_txn(true).await.unwrap();
    let kv = KvNodeStore::new(&mut txn, COLLECTION, INDEX_ID, 0);
    let record = kv
        .get_aux(codec::GRAPH, &codec::record_key(NodeId(id)))
        .await;
    record.unwrap().map(|bytes| bytes.to_vec())
}

/// Training still happens inline, on the write that crosses the threshold;
/// every write after it joins the pending set until a merge.
#[test]
fn training_happens_inline_and_later_writes_wait_for_a_merge() {
    workers::block_on(async {
        let (store, index) = saved_store().await;
        let mut read = store.new_txn(true).await.unwrap();
        let plan = index
            .merge_plan(&mut read)
            .await
            .unwrap()
            .expect("writes after training are pending");
        let trained = db::index::vector::engine::diskann::TRAIN_THRESHOLD;
        assert_eq!(plan.pending.len(), CORPUS - trained as usize);
        assert_eq!(
            plan.pending[0],
            NodeId(trained + 1),
            "the first write after training"
        );
        let (vectors, _) = corpus();
        let last = vectors.last().unwrap();
        let hits = index.search(&mut read, last, 1, None).await.unwrap();
        assert_eq!(hits[0].id.0, CORPUS as u64, "a pending vector is found");
    })
    .unwrap()
}

/// What the background task runs: the same plan-then-merge split, repeated
/// until nothing is pending.
#[test]
fn repeated_merges_drain_the_pending_set() {
    workers::block_on(async {
        let (store, index) = saved_store().await;
        let mut rounds = 0;
        while merge_due(&store, &index).await {
            rounds += 1;
            assert!(rounds < 4, "the pending set is not draining");
        }
        let mut read = store.new_txn(true).await.unwrap();
        assert!(index.merge_plan(&mut read).await.unwrap().is_none());
        let (vectors, _) = corpus();
        for i in [0usize, 600, CORPUS - 1] {
            let hits = index.search(&mut read, &vectors[i], 1, None).await.unwrap();
            assert_eq!(hits[0].id.0, i as u64 + 1);
        }
    })
    .unwrap()
}
