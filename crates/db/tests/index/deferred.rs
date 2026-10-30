//! An index on a counter is deferred: an increment owes the index an update
//! instead of moving the entry, and a read through the index checks owed
//! documents live, so it never sees a stale value (`proofs/tla/DeferredIndex.tla`).

use std::sync::Arc;

use db::{AutoCommitMutator, LensedAutoCommitFetcher, DB};
use document::NormalValue;
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use schema::{
    CType, CollectionVersion, FieldDescription, FieldKind, IndexKind, IndexedFieldDescription,
    OrderedIndexDescription,
};
use storage::corekv::{IterOptions, Iterator as _};
use storage::keys::{IndexDataStoreKey, IndexedField};
use storage::RegolithStore;

const COLLECTION: &str = "Tallies";

fn collection() -> CollectionVersion {
    CollectionVersion::new(
        COLLECTION,
        "v1",
        "col-deferred-tallies",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id(), CType::LwwRegister),
            FieldDescription::new("2", "count", FieldKind::int(), CType::PCounter),
            FieldDescription::new("3", "name", FieldKind::string(), CType::LwwRegister),
        ],
    )
}

struct Node {
    db: Arc<DB<RegolithStore>>,
    queries: Arc<dyn QueryExecutor>,
    index_id: u32,
}

impl Node {
    async fn on(store: RegolithStore, fresh: bool) -> Self {
        let db = Arc::new(DB::open(store).await.unwrap());
        let index_id = if fresh {
            db.create_collection(collection()).await.unwrap();
            db.create_index(
                COLLECTION,
                Some("by_count"),
                vec![IndexedFieldDescription {
                    name: "count".to_string(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await
            .unwrap()
            .id
        } else {
            db.require_collection(COLLECTION).unwrap().schema().indexes[0].id
        };
        let schema = db.require_collection(COLLECTION).unwrap().schema().clone();
        let queries: Arc<dyn QueryExecutor> = Arc::new(
            QueryRunner::new(LensedAutoCommitFetcher::new(Arc::clone(&db)), vec![schema])
                .with_mutator(
                    Arc::new(AutoCommitMutator::new(Arc::clone(&db))) as Arc<dyn DocMutator>
                ),
        );
        Self {
            db,
            queries,
            index_id,
        }
    }

    async fn open() -> Self {
        Self::on(RegolithStore::in_memory().unwrap(), true).await
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

    async fn create(&self, name: &str, count: i64) -> String {
        let data = self
            .run(&format!(
                r#"mutation {{ add_Tallies(input: {{name: "{name}", count: {count}}}) {{ _docID }} }}"#
            ))
            .await;
        data["add_Tallies"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn increment(&self, id: &str, by: i64) {
        self.run(&format!(
            r#"mutation {{ update_Tallies(docID: "{id}", input: {{count: {by}}}) {{ _docID }} }}"#
        ))
        .await;
    }

    /// The names a filter through the index returns, in the order it returns them.
    async fn names(&self, filter: &str, order: &str) -> Vec<String> {
        let data = self
            .run(&format!(
                "query {{ Tallies(filter: {filter}, order: {order}) {{ name }} }}"
            ))
            .await;
        data["Tallies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|doc| doc["name"].as_str().unwrap().to_owned())
            .collect()
    }

    /// How many stored entries hold `value`.
    async fn stored_at(&self, value: i64) -> usize {
        let short_id = self
            .db
            .require_collection(COLLECTION)
            .unwrap()
            .schema()
            .resolved_root_id();
        let prefix = IndexDataStoreKey::new(
            short_id,
            self.index_id,
            vec![IndexedField::ascending(NormalValue::Int(value))],
        )
        .try_bytes()
        .unwrap();
        count_keys(&self.db, prefix).await
    }

    async fn drain(&self) {
        self.db.drain_deferred_indexes().await.unwrap();
    }
}

async fn count_keys(db: &DB<RegolithStore>, prefix: Vec<u8>) -> usize {
    let txn = db.new_txn(true).await.unwrap();
    let datastore = txn.datastore().unwrap();
    let mut iter = datastore
        .iterator(IterOptions::new().with_prefix(prefix))
        .await
        .unwrap();
    let mut count = 0;
    while iter.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

#[test]
fn a_read_before_the_drain_finds_the_incremented_value() {
    workers::block_on(async {
        let node = Node::open().await;
        let id = node.create("a", 1).await;
        node.increment(&id, 4).await;

        assert_eq!(
            node.stored_at(1).await,
            1,
            "the stored entry is still owed its move"
        );
        assert_eq!(node.names("{count: {_eq: 5}}", "{count: ASC}").await, ["a"]);
        assert!(node
            .names("{count: {_eq: 1}}", "{count: ASC}")
            .await
            .is_empty());

        node.drain().await;
        assert_eq!((node.stored_at(1).await, node.stored_at(5).await), (0, 1));
        assert_eq!(node.names("{count: {_eq: 5}}", "{count: ASC}").await, ["a"]);
    })
    .unwrap();
}

#[test]
fn a_range_read_keeps_its_order_with_owed_documents() {
    workers::block_on(async {
        let node = Node::open().await;
        let a = node.create("a", 10).await;
        node.create("b", 20).await;
        node.create("c", 30).await;
        node.increment(&a, 15).await;

        assert_eq!(
            node.names("{count: {_gt: 12}}", "{count: ASC}").await,
            ["b", "a", "c"]
        );
        assert_eq!(
            node.names("{count: {_gt: 12}}", "{count: DESC}").await,
            ["c", "a", "b"]
        );
        assert_eq!(
            node.names("{count: {_lt: 22}}", "{count: ASC}").await,
            ["b"]
        );
    })
    .unwrap();
}

#[test]
fn concurrent_increments_of_an_indexed_counter_all_land() {
    workers::block_on(async {
        let node = Node::open().await;
        let id = node.create("a", 0).await;
        let conflicts = telemetry::storage_event_count(telemetry::StorageEvent::AbortConflict);

        futures::future::join_all((0..32).map(|_| node.increment(&id, 1))).await;

        assert_eq!(
            telemetry::storage_event_count(telemetry::StorageEvent::AbortConflict),
            conflicts,
            "an increment of an indexed counter reads nothing a concurrent one writes"
        );
        assert_eq!(
            node.names("{count: {_eq: 32}}", "{count: ASC}").await,
            ["a"]
        );
        node.drain().await;
        assert_eq!((node.stored_at(0).await, node.stored_at(32).await), (0, 1));
    })
    .unwrap();
}

#[test]
fn a_deleted_document_leaves_the_index() {
    workers::block_on(async {
        let node = Node::open().await;
        let id = node.create("a", 3).await;
        node.increment(&id, 1).await;
        node.run(&format!(
            r#"mutation {{ delete_Tallies(docID: "{id}") {{ _docID }} }}"#
        ))
        .await;

        assert!(node
            .names("{count: {_eq: 4}}", "{count: ASC}")
            .await
            .is_empty());
        node.drain().await;
        assert_eq!((node.stored_at(3).await, node.stored_at(4).await), (0, 0));
    })
    .unwrap();
}

#[test]
fn dropping_the_index_leaves_no_record_and_no_log() {
    workers::block_on(async {
        let node = Node::open().await;
        let id = node.create("a", 3).await;
        node.increment(&id, 1).await;
        let short_id = node
            .db
            .require_collection(COLLECTION)
            .unwrap()
            .schema()
            .resolved_root_id();

        node.db.drop_index(COLLECTION, "by_count").await.unwrap();

        let records = storage::keys::index_position_prefix(short_id, node.index_id);
        let log = storage::keys::index_pending_log(short_id, node.index_id);
        assert_eq!(count_keys(&node.db, records).await, 0);
        assert_eq!(count_keys(&node.db, log.entry_prefix().to_vec()).await, 0);
    })
    .unwrap();
}

#[test]
fn work_owed_at_a_restart_is_drained_after_it() {
    workers::block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let id = {
            let node = Node::on(RegolithStore::open(dir.path()).unwrap(), true).await;
            let id = node.create("a", 1).await;
            node.increment(&id, 2).await;
            node.db.close().await.unwrap();
            id
        };
        let node = Node::on(RegolithStore::open(dir.path()).unwrap(), false).await;
        assert_eq!(
            node.names("{count: {_eq: 3}}", "{count: ASC}").await,
            ["a"],
            "{id}"
        );
        node.drain().await;
        assert_eq!((node.stored_at(1).await, node.stored_at(3).await), (0, 1));
    })
    .unwrap();
}
