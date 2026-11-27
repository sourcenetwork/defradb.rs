//! A vector index declared `maintenance: DEFERRED` leaves its graph to the
//! drain: a write puts one pending entry, a search scores the pending entries
//! beside the graph, and the drain applies them
//! (`proofs/tla/DeferredVector.tla`).

use std::sync::Arc;

use db::index::vector::engine::ann::Neighbor;
use db::index::vector::index::{VectorIndex, LIVE_EPOCH};
use db::index::vector::pending::{
    count_pending, put_pending, PendingEntry, PendingOp, PENDING, PENDING_LIMIT,
};
use db::index::vector::store::{KvNodeStore, NodeId, VectorNodeStore};
use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::DocID;
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use storage::corekv::Store;
use storage::RegolithStore;

const COLLECTION: &str = "User";

fn sdl(maintenance: &str) -> String {
    format!(
        "type User {{ name: String vector: [Float32!] @index(vector: {{dimensions: 3, {maintenance}hnsw: {{metric: COSINE}}}}) }}"
    )
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    queries: Arc<dyn QueryExecutor>,
}

impl Node {
    async fn on(store: RegolithStore, sdl: Option<&str>) -> Self {
        let db = Arc::new(DB::open(store).await.unwrap());
        if let Some(sdl) = sdl {
            db.add_collection(&defra_core::RequestContext::ambient(), sdl)
                .await
                .unwrap();
        }
        let schema = db.require_collection(COLLECTION).unwrap().schema().clone();
        let queries: Arc<dyn QueryExecutor> = Arc::new(
            QueryRunner::new(LensedAutoCommitFetcher::new(Arc::clone(&db)), vec![schema])
                .with_mutator(
                    Arc::new(AutoCommitMutator::new(Arc::clone(&db))) as Arc<dyn DocMutator>
                ),
        );
        Self { db, queries }
    }

    async fn deferred() -> Self {
        Self::on(
            RegolithStore::in_memory().unwrap(),
            Some(&sdl("maintenance: DEFERRED, ")),
        )
        .await
    }

    async fn run(&self, request: &str) -> serde_json::Value {
        let response = self.queries.execute(QueryRequest::new(request)).await;
        assert!(
            response.errors.is_empty(),
            "{request}: {:?}",
            response.errors
        );
        response.data.unwrap()
    }

    async fn create(&self, name: &str, vector: [f32; 3]) -> String {
        let data = self
            .run(&format!(
                r#"mutation {{ add_User(input: {{name: "{name}", vector: {vector:?}}}) {{ _docID }} }}"#
            ))
            .await;
        data["add_User"][0]["_docID"].as_str().unwrap().to_owned()
    }

    async fn update(&self, id: &str, vector: [f32; 3]) {
        self.run(&format!(
            r#"mutation {{ update_User(docID: "{id}", input: {{vector: {vector:?}}}) {{ _docID }} }}"#
        ))
        .await;
    }

    async fn delete(&self, id: &str) {
        self.run(&format!(
            r#"mutation {{ delete_User(docID: ["{id}"]) {{ _docID }} }}"#
        ))
        .await;
    }

    /// The names nearest `[1, 0, 0]`, nearest first, through the index.
    async fn nearest(&self, limit: usize) -> Vec<String> {
        let data = self
            .run(&format!(
                "query {{ User(order: {{_alias: {{sim: DESC}}}}, limit: {limit}) {{ name sim: SIMILARITY(vector: {{vector: [1, 0, 0]}}) }} }}"
            ))
            .await;
        data["User"]
            .as_array()
            .unwrap()
            .iter()
            .map(|doc| doc["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn index(&self) -> (u32, u32) {
        let collection = self.db.require_collection(COLLECTION).unwrap();
        (
            collection.resolved_root_id(),
            collection.schema().indexes[0].id,
        )
    }

    /// Pending entries and live graph nodes, read on a snapshot.
    async fn stored(&self) -> (u64, usize) {
        let (short_id, index_id) = self.index();
        let txn = self.db.new_txn(true).await.unwrap();
        let mut view = txn.datastore().unwrap();
        let store = KvNodeStore::new(&mut view, short_id, index_id, LIVE_EPOCH);
        let pending = count_pending(&store).await.unwrap();
        let mut nodes = 0;
        store
            .iterate_nodes(|_| {
                nodes += 1;
                Ok(())
            })
            .await
            .unwrap();
        (pending, nodes)
    }

    /// The `k` nearest the index itself returns. A query's results stay right
    /// even when the index returns too few, so they alone cannot show that the
    /// index found its pending writes.
    async fn found_by_index(&self, k: usize) -> Vec<Neighbor> {
        let collection = self.db.require_collection(COLLECTION).unwrap();
        let index = VectorIndex::try_new(
            collection.resolved_root_id(),
            collection.schema().indexes[0].clone(),
        )
        .unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let mut view = txn.datastore().unwrap();
        index
            .search(&mut view, &[1.0f64, 0.0, 0.0], k, None)
            .await
            .unwrap()
    }

    async fn short_id(&self, doc_id: &str) -> u64 {
        let collection = self.db.require_collection(COLLECTION).unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let id = DocID::from_string(doc_id).unwrap();
        collection
            .resolve_doc_short_id(&systemstore, &id)
            .await
            .unwrap()
            .unwrap()
    }

    async fn drain(&self) {
        self.db.drain_deferred_indexes().await.unwrap();
    }
}

#[test]
fn a_deferred_write_is_found_before_the_drain_applies_it() {
    workers::block_on(async {
        let node = Node::deferred().await;
        node.create("x", [1.0, 0.0, 0.0]).await;
        node.create("xy", [0.9, 0.4, 0.0]).await;
        node.create("y", [0.0, 1.0, 0.0]).await;

        assert_eq!(node.stored().await, (3, 0), "the graph is left alone");
        assert_eq!(
            node.found_by_index(3).await.len(),
            3,
            "the index scores its pending writes"
        );
        assert_eq!(node.nearest(2).await, ["x", "xy"]);

        node.drain().await;
        assert_eq!(node.stored().await, (0, 3), "the drain applied every write");
        assert_eq!(node.nearest(2).await, ["x", "xy"]);
    })
    .unwrap();
}

#[test]
fn an_update_and_a_delete_before_the_drain_hide_the_stale_graph() {
    workers::block_on(async {
        let node = Node::deferred().await;
        let best = node.create("d0", [1.0, 0.0, 0.0]).await;
        node.create("d1", [0.9, 0.1, 0.0]).await;
        node.create("d2", [0.8, 0.3, 0.1]).await;
        let moved = node.create("d4", [0.0, 1.0, 0.0]).await;
        node.create("d7", [-1.0, 0.0, 0.0]).await;
        node.drain().await;
        assert_eq!(node.stored().await, (0, 5));

        node.update(&moved, [0.95, 0.05, 0.0]).await;
        node.delete(&best).await;
        assert_eq!(node.stored().await.0, 2, "one entry per written node");
        let found = node.found_by_index(10).await;
        assert_eq!(found.len(), 4, "the deleted node's graph hit is dropped");
        let (moved_id, deleted_id) = (node.short_id(&moved).await, node.short_id(&best).await);
        assert_eq!(
            found
                .iter()
                .filter(|neighbour| neighbour.id == NodeId(moved_id))
                .count(),
            1,
            "the updated node appears once"
        );
        assert_eq!(
            found[0].id,
            NodeId(moved_id),
            "the updated node ranks by its new vector"
        );
        assert!(
            found
                .iter()
                .all(|neighbour| neighbour.id != NodeId(deleted_id)),
            "the deleted node is absent from the index results"
        );
        assert_eq!(node.nearest(3).await, ["d4", "d1", "d2"]);

        node.drain().await;
        assert_eq!(node.stored().await.0, 0);
        assert_eq!(node.nearest(3).await, ["d4", "d1", "d2"]);
    })
    .unwrap();
}

#[test]
fn a_sync_index_writes_no_pending_entry() {
    workers::block_on(async {
        let node = Node::on(RegolithStore::in_memory().unwrap(), Some(&sdl(""))).await;
        node.create("x", [1.0, 0.0, 0.0]).await;
        assert_eq!(node.stored().await, (0, 1));
    })
    .unwrap();
}

#[test]
fn a_write_past_the_pending_limit_updates_the_graph_itself() {
    workers::block_on(async {
        let node = Node::deferred().await;
        let early = node.create("early", [0.0, 1.0, 0.0]).await;
        let (short_id, index_id) = node.index();
        let backlog = node.db.deferred_drains().backlogs().get(short_id, index_id);
        assert_eq!(backlog.pending(), 1, "the commit counted its write");
        backlog.add(PENDING_LIMIT);

        node.create("late", [1.0, 0.0, 0.0]).await;
        assert_eq!(node.stored().await, (1, 1), "the late write went inline");
        node.update(&early, [0.9, 0.1, 0.0]).await;
        assert_eq!(
            node.stored().await,
            (0, 2),
            "an inline write supersedes the node's pending entry"
        );
        assert_eq!(node.nearest(2).await, ["late", "early"]);
    })
    .unwrap();
}

#[test]
fn concurrent_deferred_writers_do_not_conflict() {
    workers::block_on(async {
        let node = Node::deferred().await;
        let stats = |db: &DB<RegolithStore>| {
            db.store()
                .transaction_stats_handle()
                .expect("regolith reports its transaction statistics")
                .snapshot()
                .conflicts
        };
        let before = stats(&node.db);
        let writes = (0..8).map(|i| {
            let node = &node;
            async move {
                node.create(&format!("d{i}"), [1.0, i as f32 / 10.0, 0.0])
                    .await
            }
        });
        futures::future::join_all(writes).await;
        assert_eq!(stats(&node.db), before, "pending puts are blind");
        assert_eq!(node.stored().await, (8, 0));
        assert_eq!(
            node.nearest(8).await,
            ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"]
        );
    })
    .unwrap();
}

#[test]
fn a_restart_counts_the_stored_backlog_before_the_next_write() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let (short_id, index_id);
        {
            let store = RegolithStore::open(dir.path()).unwrap();
            let node = Node::on(store, Some(&sdl("maintenance: DEFERRED, "))).await;
            (short_id, index_id) = node.index();
            let txn = node.db.new_txn(false).await.unwrap();
            {
                let mut view = txn.datastore().unwrap();
                let mut store = KvNodeStore::new(&mut view, short_id, index_id, LIVE_EPOCH);
                store
                    .put_aux(
                        PENDING,
                        &db::index::vector::pending::entry::node_key(NodeId(1)),
                        &[],
                    )
                    .await
                    .unwrap();
                for id in 2..=PENDING_LIMIT + 1 {
                    put_pending(
                        &mut store,
                        &PendingEntry {
                            id: NodeId(id),
                            op: PendingOp::Upsert,
                            vector: vec![1.0, 0.0, 0.0],
                        },
                    )
                    .await
                    .unwrap();
                }
            }
            txn.commit().await.unwrap();
            node.db.close().await.unwrap();
        }
        let node = Node::on(RegolithStore::open(dir.path()).unwrap(), None).await;
        let backlog = node.db.deferred_drains().backlogs().get(short_id, index_id);
        assert_eq!(node.stored().await, (PENDING_LIMIT + 1, 0));
        assert!(
            node.db.drain_deferred_indexes().await.is_err(),
            "the malformed first entry fails"
        );
        assert_eq!(
            backlog.pending(),
            PENDING_LIMIT + 1,
            "the stored count survives a failed first drain"
        );

        node.create("after", [1.0, 0.0, 0.0]).await;
        assert_eq!(
            node.stored().await,
            (PENDING_LIMIT, 1),
            "a write past the stored limit updates the graph itself"
        );
    })
    .unwrap();
}

#[test]
fn a_rebuild_applies_the_pending_writes_rather_than_clearing_them() {
    workers::block_on(async {
        let node = Node::deferred().await;
        let (short_id, index_id) = node.index();
        let backlog = node.db.deferred_drains().backlogs().get(short_id, index_id);
        // Inline writes until enough tombstones make a rebuild due.
        backlog.add(PENDING_LIMIT);
        let mut ids = Vec::new();
        for i in 0..80 {
            let angle = i as f32 * 0.1;
            ids.push(
                node.create(&format!("d{i}"), [angle.cos(), angle.sin(), 0.5])
                    .await,
            );
        }
        for id in &ids[10..] {
            node.delete(id).await;
        }
        backlog.sub(u64::MAX);
        node.create("pending", [1.0, 0.0, 0.0]).await;
        assert_eq!(node.stored().await, (1, 10));

        node.db.rebuild_due_vector_indexes().await.unwrap();
        assert_eq!(
            node.stored().await,
            (0, 11),
            "the rebuild kept the pending write"
        );
        assert_eq!(backlog.pending(), 0, "the rebuild cleared the backlog");
        node.create("next", [0.0, 1.0, 0.0]).await;
        assert_eq!(
            node.stored().await,
            (1, 11),
            "the next write defers again after the rebuild"
        );
        assert_eq!(node.nearest(1).await, ["pending"]);
    })
    .unwrap();
}

#[test]
fn an_update_counts_one_pending_write() {
    workers::block_on(async {
        let node = Node::deferred().await;
        let id = node.create("x", [1.0, 0.0, 0.0]).await;
        let (short_id, index_id) = node.index();
        let backlog = node.db.deferred_drains().backlogs().get(short_id, index_id);
        assert_eq!(backlog.pending(), 1, "the create counted one write");

        node.update(&id, [0.0, 1.0, 0.0]).await;
        assert_eq!(node.stored().await.0, 1, "one entry per written node");
        assert_eq!(backlog.pending(), 2, "the update counted one write");
    })
    .unwrap();
}
