//! Two updates of disjoint fields of one document never conflict, whatever
//! the update's shape: selected by ID or by filter, returning any fields, on a
//! migrated collection, on a downsample source, or incrementing a counter.
//! Each update reads only the parts it decides on (`query::update_parts`).
//!
//! Each case runs both updates in explicit transactions that begin before
//! either writes, so the second commit meets the first's writes.

use std::sync::Arc;

use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
use query::{QueryExecutor, QueryRequest};
use serde_json::Value;
use storage::RegolithStore;

struct Node {
    db: Arc<DB<RegolithStore>>,
    queries: Arc<dyn QueryExecutor>,
}

impl Node {
    async fn new(sdl: &str) -> Self {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        for schema in query::parse_sdl(sdl).unwrap() {
            db.create_collection(schema).await.unwrap();
        }
        let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
        let queries: Arc<dyn QueryExecutor> =
            Arc::new(query::QueryRunner::with_arc_registry_and_provider(
                LensedAutoCommitFetcher::new(Arc::clone(&db)),
                DbCollectionProvider::new_arc(Arc::clone(&db)),
                registry,
            ));
        Self { db, queries }
    }

    /// Run `mutation` in a transaction of its own and commit it.
    async fn commit(&self, mutation: &str) -> Value {
        let txn = self.queries.begin_txn(false).await.unwrap();
        let response = self
            .queries
            .execute_in_txn(QueryRequest::new(mutation), &txn)
            .await;
        assert!(!response.has_errors(), "{mutation}: {:?}", response.errors);
        self.queries.commit_txn(&txn).await.unwrap();
        response.data.unwrap()
    }

    /// Create one document and return its ID.
    async fn create(&self, mutation: &str) -> String {
        let created = self.commit(mutation).await;
        let (_, value) = created.as_object().unwrap().iter().next().unwrap();
        let doc = if value.is_array() { &value[0] } else { value };
        doc["_docID"].as_str().unwrap().to_string()
    }

    /// Run `first` and `second` in transactions that both begin before
    /// either writes, then commit them in that order.
    async fn commit_both(&self, first: &str, second: &str) -> Result<(), String> {
        let one = self.queries.begin_txn(false).await.unwrap();
        let two = self.queries.begin_txn(false).await.unwrap();
        for (txn, mutation) in [(&one, first), (&two, second)] {
            let response = self
                .queries
                .execute_in_txn(QueryRequest::new(mutation), txn)
                .await;
            assert!(!response.has_errors(), "{mutation}: {:?}", response.errors);
        }
        self.queries.commit_txn(&one).await.unwrap();
        self.queries
            .commit_txn(&two)
            .await
            .map_err(|error| format!("{error:?}"))
    }

    async fn both_commit(&self, first: &str, second: &str) {
        if let Err(error) = self.commit_both(first, second).await {
            panic!("{second} conflicted with {first}: {error}");
        }
    }

    async fn query(&self, request: &str) -> Value {
        let response = self.queries.execute(QueryRequest::new(request)).await;
        assert!(!response.has_errors(), "{request}: {:?}", response.errors);
        response.data.unwrap()
    }
}

const USERS: &str = "type Users { name: String  age: Int  email: String }";
const ANN: &str =
    r#"mutation { add_Users(input: {name: "ann", age: 30, email: "ann@old"}) { _docID } }"#;

async fn assert_ann(node: &Node, age: i64, email: &str) {
    let users = node.query("{ Users { name age email } }").await;
    assert_eq!(
        users["Users"],
        serde_json::json!([{ "name": "ann", "age": age, "email": email }])
    );
}

#[test]
fn filtered_updates_of_disjoint_fields_both_commit() {
    workers::block_on(async {
    let node = Node::new(USERS).await;
    node.create(ANN).await;
    node.both_commit(
        r#"mutation { update_Users(filter: {name: {_eq: "ann"}}, input: {age: 31}) { _docID } }"#,
        r#"mutation { update_Users(filter: {name: {_eq: "ann"}}, input: {email: "ann@new"}) { _docID } }"#,
    )
    .await;
    assert_ann(&node, 31, "ann@new").await;
})
    .unwrap()
}

#[test]
fn updates_of_every_document_with_disjoint_fields_both_commit() {
    workers::block_on(async {
        let node = Node::new(USERS).await;
        node.create(ANN).await;
        node.both_commit(
            r#"mutation { update_Users(input: {age: 31}) { _docID } }"#,
            r#"mutation { update_Users(input: {email: "ann@new"}) { _docID } }"#,
        )
        .await;
        assert_ann(&node, 31, "ann@new").await;
    })
    .unwrap()
}

#[test]
fn updates_returning_other_fields_both_commit() {
    workers::block_on(async {
    let node = Node::new(USERS).await;
    let id = node.create(ANN).await;
    node.both_commit(
        &format!(r#"mutation {{ update_Users(docID: "{id}", input: {{age: 31}}) {{ name age email }} }}"#),
        &format!(r#"mutation {{ update_Users(docID: "{id}", input: {{email: "ann@new"}}) {{ _docID name age }} }}"#),
    )
    .await;
    assert_ann(&node, 31, "ann@new").await;
})
    .unwrap()
}

/// Two writes of one field are one winner by design: projection never lets
/// both commit.
#[test]
fn updates_of_the_same_field_still_conflict() {
    workers::block_on(async {
        let node = Node::new(USERS).await;
        let id = node.create(ANN).await;
        let outcome = node
            .commit_both(
                &format!(
                    r#"mutation {{ update_Users(docID: "{id}", input: {{age: 31}}) {{ _docID }} }}"#
                ),
                &format!(
                    r#"mutation {{ update_Users(docID: "{id}", input: {{age: 32}}) {{ _docID }} }}"#
                ),
            )
            .await;
        assert!(outcome.is_err(), "two writes of one field both committed");
        assert_ann(&node, 31, "ann@old").await;
    })
    .unwrap()
}

/// A filter's fields are decided on: a concurrent change of one conflicts.
#[test]
fn an_update_conflicts_with_a_change_of_the_field_its_filter_compares() {
    workers::block_on(async {
    let node = Node::new(USERS).await;
    node.create(ANN).await;
    let outcome = node
        .commit_both(
            r#"mutation { update_Users(filter: {name: {_eq: "ann"}}, input: {name: "bea"}) { _docID } }"#,
            r#"mutation { update_Users(filter: {name: {_eq: "ann"}}, input: {age: 31}) { _docID } }"#,
        )
        .await;
    assert!(
        outcome.is_err(),
        "an update whose filter matched a renamed document committed"
    );
})
    .unwrap()
}

#[test]
fn updates_of_a_migrated_collection_both_commit() {
    workers::block_on(async {
    let node = Node::new(USERS).await;
    let migration = lens::LensConfig::new(
        "ignored-source",
        "ignored-destination",
        lens::LensModule::from_bytes(b"\0asm\x01\0\0\0".to_vec()),
    );
    node.db
        .patch_collection_with_migration(
            "Users",
            r#"[{"op": "add", "path": "/Users/Fields/-", "value": {"Name": "nickname", "Kind": "String"}}]"#,
            Some(migration),
            None,
        )
        .await
        .unwrap();
    let id = node.create(ANN).await;
    node.both_commit(
        &format!(r#"mutation {{ update_Users(docID: "{id}", input: {{age: 31}}) {{ _docID }} }}"#),
        &format!(r#"mutation {{ update_Users(docID: "{id}", input: {{email: "ann@new"}}) {{ _docID }} }}"#),
    )
    .await;
    assert_ann(&node, 31, "ann@new").await;
})
    .unwrap()
}

const METRIC: &str = "type Metric { label: String  note: String  ts: DateTime  value: Int }";
const ROLLUP: &str = r#"
type MetricRollup @downsample(interval: "1000ms", timeField: "ts") {
  label: String
  source_doc_id: String
  source_height: Int
  window_start: DateTime
  window_end: DateTime
  count: Int
  sum: Int
  avg: Float
  min: Int
  max: Int
}
"#;

/// A raw downsample source is checked only when its time or measure changes,
/// so updates of its other fields decide on nothing else.
#[test]
fn updates_of_a_downsample_source_both_commit() {
    workers::block_on(async {
    let node = Node::new(METRIC).await;
    let known: rapidhash::RapidHashSet<String> =
        node.db.list_collections().unwrap().into_iter().collect();
    let source = query::parse_query_with_limits(
        "query { Metric { label ts value } }",
        None,
        query::QueryLimits::default(),
    )
    .unwrap();
    for mut rollup in query::parse_sdl_with_known_types(ROLLUP, known).unwrap() {
        rollup.downsample_source = Some(schema::QuerySource::new(query::select_to_go_json(
            &source[0],
        )));
        node.db.validate_downsample_collection(&rollup).unwrap();
        node.db.create_collection(rollup).await.unwrap();
    }
    let id = node
        .create(r#"mutation { add_Metric(input: {label: "cpu", note: "a", ts: "2026-01-01T00:00:00Z", value: 1}) { _docID } }"#)
        .await;
    node.both_commit(
        &format!(
            r#"mutation {{ update_Metric(docID: "{id}", input: {{label: "gpu"}}) {{ _docID }} }}"#
        ),
        &format!(
            r#"mutation {{ update_Metric(docID: "{id}", input: {{note: "b"}}) {{ _docID }} }}"#
        ),
    )
    .await;
    let metrics = node.query("{ Metric { label note } }").await;
    assert_eq!(
        metrics["Metric"],
        serde_json::json!([{ "label": "gpu", "note": "b" }])
    );
})
    .unwrap()
}

const COUNTED: &str = "type Counted { label: String  hits: Int @crdt(type: pncounter) }";

#[test]
fn counter_updates_returning_the_counter_both_commit() {
    workers::block_on(async {
    let node = Node::new(COUNTED).await;
    let id = node
        .create(r#"mutation { add_Counted(input: {label: "a", hits: 0}) { _docID } }"#)
        .await;
    node.both_commit(
        &format!(r#"mutation {{ update_Counted(docID: "{id}", input: {{hits: 1}}) {{ hits }} }}"#),
        &format!(
            r#"mutation {{ update_Counted(docID: "{id}", input: {{hits: 2}}) {{ label hits }} }}"#
        ),
    )
    .await;
    node.both_commit(
        &format!(r#"mutation {{ update_Counted(docID: "{id}", input: {{hits: 4}}) {{ hits }} }}"#),
        &format!(r#"mutation {{ update_Counted(docID: "{id}", input: {{label: "b"}}) {{ label hits }} }}"#),
    )
    .await;
    let counted = node.query("{ Counted { label hits } }").await;
    assert_eq!(
        counted["Counted"],
        serde_json::json!([{ "label": "b", "hits": 7 }])
    );
})
    .unwrap()
}
