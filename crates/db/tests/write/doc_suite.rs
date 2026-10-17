use crate::common::fixture::make_test_db_with_bus;
use crate::common::owned::in_txn;
use crate::common::schema::test_collection;
use acp::DocumentACP;
use acp::LocalDocumentACP;
use acp::MemoryAcpStore;
use db::database::DB;
use db::index::IndexManager;
use db::txn::registry::DbTransactionRegistry;
use db::write::doc::*;
use document::DocID;
use document::Document;
use document::NormalValue;
use events::EventName;
use kovan_queue::seg_queue::SegQueue;
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use query::txn::TransactionRegistry;
use rapidhash::HashSetExt;
use schema::CType;
use schema::CollectionVersion;
use schema::FieldDescription;
use schema::FieldKind;
use schema::PolicyDescription;
use std::sync::Arc;
use storage::corekv::OnCommit;
use storage::index::IndexIterator;
use storage::RegolithStore;

fn branchable_collection() -> CollectionVersion {
    CollectionVersion::new(
        "Branchable",
        "branchable-v1",
        "col-branchable",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "x", FieldKind::int()),
        ],
    )
    .as_branchable()
}

struct FailingCollectionSigner;

impl defra_core::signing::RemoteSigner for FailingCollectionSigner {
    fn sign_sync(
        &self,
        data: &[u8],
        _authorization: Option<&defra_core::signing::SigningAuthorization>,
    ) -> Result<Vec<u8>, String> {
        let block =
            defra_core::block::Block::from_dag_cbor(data).map_err(|error| error.to_string())?;
        if matches!(block.delta, defra_core::block::CrdtDelta::Collection(_)) {
            return Err("injected collection signing failure".to_string());
        }
        Ok(vec![0; 64])
    }
}

fn failing_collection_signing_config() -> defra_core::signing::SigningConfig {
    defra_core::signing::SigningConfig {
        key_type: defra_core::signing::SigningKeyType::Ed25519,
        private_key_bytes: Vec::new(),
        public_key_bytes: Vec::new(),
        public_key_hex: "test".to_string(),
        remote_signer: Some(Arc::new(FailingCollectionSigner)),
        signing_authorization: None,
    }
}

#[test]
fn branchable_create_rolls_back_when_collection_block_fails() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(branchable_collection()).await.unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));

        defra_core::signing::set_signing_config(Some(failing_collection_signing_config()));
        let result = mutator
            .create(
                "Branchable",
                Document::from_json_str(r#"{"x": 1}"#).unwrap(),
            )
            .await;
        defra_core::signing::set_signing_config(None);

        let error = result.expect_err("collection block failure must fail create");
        assert!(error
            .to_string()
            .contains("injected collection signing failure"));

        let fetcher = db::LensedAutoCommitFetcher::new(Arc::clone(&db));
        assert!(fetcher.get_all("Branchable").await.unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn branchable_update_rolls_back_when_collection_block_fails() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(branchable_collection()).await.unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator
            .create(
                "Branchable",
                Document::from_json_str(r#"{"x": 1}"#).unwrap(),
            )
            .await
            .unwrap();
        let mut doc = mutator
            .get_for_update("Branchable", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("x", NormalValue::Int(2));

        defra_core::signing::set_signing_config(Some(failing_collection_signing_config()));
        let result = mutator
            .update(
                "Branchable",
                doc,
                std::iter::once("x".to_string()).collect(),
            )
            .await;
        defra_core::signing::set_signing_config(None);

        let error = result.expect_err("collection block failure must fail update");
        assert!(error
            .to_string()
            .contains("injected collection signing failure"));

        let persisted = mutator
            .get_for_update("Branchable", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.get("x"), Some(&NormalValue::Int(1)));
    })
    .unwrap()
}

fn counter_collection() -> CollectionVersion {
    CollectionVersion::new(
        "Counters",
        "cv1",
        "col-counters",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "count", FieldKind::int()).with_crdt_type(CType::PnCounter),
        ],
    )
}

fn float32_counter_collection() -> CollectionVersion {
    CollectionVersion::new(
        "FloatCounters",
        "fcv1",
        "col-float-counters",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "points", FieldKind::float32())
                .with_crdt_type(CType::PnCounter),
        ],
    )
}

/// Read a counter's committed value: its part of the document value, the
/// counter's only stored copy, through a fresh read transaction.
async fn read_counter_store(
    db: &Arc<DB<RegolithStore>>,
    collection_name: &str,
    doc_id: &str,
    field: &str,
) -> i64 {
    read_counter_store_opt(db, collection_name, doc_id, field)
        .await
        .expect("the counter has a committed value")
}

#[test]
fn auto_commit_float32_counter_accumulates_with_float32_precision() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(float32_counter_collection())
            .await
            .unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&db));

        let mut doc = Document::new();
        doc.set("points", NormalValue::Float32(0.0));
        let doc_id = mutator.create("FloatCounters", doc).await.unwrap().doc_id;

        for (value, delta) in [(10.1f32, 10.1f32), (10.1f32 + 10.2f32, 10.2f32)] {
            let mut doc = mutator
                .get_for_update("FloatCounters", &doc_id)
                .await
                .unwrap()
                .unwrap();
            doc.set("points", NormalValue::Float32(value));
            doc.set_counter_delta("points".into(), NormalValue::Float32(delta));
            mutator
                .update(
                    "FloatCounters",
                    doc,
                    std::iter::once("points".to_string()).collect(),
                )
                .await
                .unwrap();
        }

        let doc = mutator
            .get_for_update("FloatCounters", &doc_id)
            .await
            .unwrap()
            .unwrap();
        let expected = 10.1f32 + 10.2f32;
        assert_eq!(
            doc.get("points"),
            Some(&NormalValue::Float64(f64::from(expected)))
        );
    })
    .unwrap()
}

#[test]
fn alias_reads_and_mutations_use_the_canonical_doc_id() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection()).await.unwrap();
        let mutator = db::AutoCommitMutator::new(db.clone());
        let mut doc = Document::new();
        doc.set("x", NormalValue::Int(1));
        let created = mutator.create("TestDoc", doc).await.unwrap();
        let canonical_doc_id = created.doc_id;
        let alias =
            DocID::new_v0(defra_core::block::generate_cid_from_bytes(b"imported-alias").unwrap());

        let txn = db.new_txn(false).await.unwrap();
        {
            let systemstore = txn.systemstore().unwrap();
            let doc_ref = db::docid::map::get_doc_ref(&systemstore, &canonical_doc_id.to_string())
                .await
                .unwrap()
                .unwrap();
            db::docid::map::set_doc_id_alias(
                &systemstore,
                doc_ref.collection_short_id,
                doc_ref.doc_short_id,
                &alias.to_string(),
            )
            .await
            .unwrap();
        }
        txn.commit().await.unwrap();

        let mut fetched = mutator
            .get_for_update("TestDoc", &alias)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.id(), Some(&canonical_doc_id));
        fetched.set_id(alias.clone());
        fetched.set("x", NormalValue::Int(2));
        let updated = mutator
            .update(
                "TestDoc",
                fetched,
                std::iter::once("x".to_string()).collect(),
            )
            .await
            .unwrap();
        assert_eq!(updated.document.id(), Some(&canonical_doc_id));

        let deleted = mutator.delete("TestDoc", &alias).await.unwrap();
        assert!(deleted.existed);
        assert_eq!(deleted.doc_id, canonical_doc_id);
        let txn = db.new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let owners = db::docid::map::get_doc_ids_for_block(
            &systemstore,
            &deleted.commit_cid.unwrap().to_string(),
        )
        .await
        .unwrap();
        assert_eq!(owners, vec![canonical_doc_id.to_string()]);
    })
    .unwrap()
}

#[test]
fn explicit_txn_counter_increment_advances_the_counter() {
    workers::block_on(async {
        // Regression for #1021: an explicit-transaction counter increment must
        // reach the counter's committed value, driven through the registry commit.

        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        // Create the doc (count = 5) in an explicit txn, commit via the registry.
        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");
                let create_doc = Document::from_json_str(r#"{"count": 5}"#).expect("doc");
                let created = mutator
                    .create("Counters", create_doc)
                    .await
                    .expect("create");
                created.doc_id.to_string()
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            5,
            "create must store the counter at 5"
        );

        // Increment by 3 in a fresh explicit txn, commit via the registry.
        let handle = registry.begin(false).await.expect("begin");
        in_txn(&registry, &handle, {
            let doc_id = doc_id.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");
                    let mut update_doc = Document::from_json_str(r#"{"count": 8}"#).expect("doc");
                    update_doc.set_id(document::DocID::from_string(&doc_id).expect("doc id"));
                    update_doc
                        .set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                    let mut modified = rapidhash::RapidHashSet::new();
                    modified.insert("count".to_string());
                    mutator
                        .update("Counters", update_doc, modified)
                        .await
                        .expect("update");
                })
            }
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            8,
            "an explicit-txn increment must advance the counter (#1021)"
        );
    })
    .unwrap()
}

/// PCounter (increment-only) collection: an increment must be applied once,
/// whatever value the caller's copy of the document carries (#1044 BUG 1).
fn pcounter_collection() -> CollectionVersion {
    CollectionVersion::new(
        "PCounters",
        "pcv1",
        "col-pcounters",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "count", FieldKind::int()).with_crdt_type(CType::PCounter),
        ],
    )
}

/// Regression for #1044 BUG 1 (PCounter explicit-txn double-apply): create a
/// PCounter doc at 5, then increment +3 in a separate explicit txn whose
/// document copy already says 8. The counter must end at 8, NOT 11.
#[test]
fn explicit_txn_pcounter_increment_no_double_apply() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(pcounter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        // Create the doc (count = 5) in an explicit txn, commit via the registry.
        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");
                let create_doc = Document::from_json_str(r#"{"count": 5}"#).expect("doc");
                let created = mutator
                    .create("PCounters", create_doc)
                    .await
                    .expect("create");
                created.doc_id.to_string()
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "PCounters", &doc_id, "count").await,
            5,
            "create must store the PCounter at 5"
        );

        // Increment by 3 in a fresh explicit txn (provisional blob = 8).
        let handle = registry.begin(false).await.expect("begin");
        in_txn(&registry, &handle, {
            let doc_id = doc_id.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");
                    let mut update_doc = Document::from_json_str(r#"{"count": 8}"#).expect("doc");
                    update_doc.set_id(document::DocID::from_string(&doc_id).expect("doc id"));
                    update_doc
                        .set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                    let mut modified = rapidhash::RapidHashSet::new();
                    modified.insert("count".to_string());
                    mutator
                        .update("PCounters", update_doc, modified)
                        .await
                        .expect("update");
                })
            }
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "PCounters", &doc_id, "count").await,
            8,
            "PCounter increment must NOT double-apply: 8, not 11 (#1044)"
        );
    })
    .unwrap()
}

/// An explicit txn that increments counters on TWO different docs must, after
/// commit, leave BOTH counters advanced.
#[test]
fn explicit_txn_multi_doc_counter_increments_advance_both() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        // Seed two docs (count = 10 and count = 20) in one explicit txn.
        let handle = registry.begin(false).await.expect("begin");
        let (doc_a, doc_b) = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");
                let doc_a = mutator
                    .create(
                        "Counters",
                        Document::from_json_str(r#"{"count": 10}"#).unwrap(),
                    )
                    .await
                    .expect("create a")
                    .doc_id
                    .to_string();
                let doc_b = mutator
                    .create(
                        "Counters",
                        Document::from_json_str(r#"{"count": 20}"#).unwrap(),
                    )
                    .await
                    .expect("create b")
                    .doc_id
                    .to_string();
                (doc_a, doc_b)
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit creates");

        // Increment BOTH docs in a single explicit txn.
        let handle = registry.begin(false).await.expect("begin");
        in_txn(&registry, &handle, {
            let doc_a = doc_a.clone();
            let doc_b = doc_b.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");

                    let mut up_a = Document::from_json_str(r#"{"count": 11}"#).unwrap();
                    up_a.set_id(document::DocID::from_string(&doc_a).unwrap());
                    up_a.set_counter_delta("count".to_string(), document::NormalValue::Int(1));
                    let mut mod_a = rapidhash::RapidHashSet::new();
                    mod_a.insert("count".to_string());
                    mutator
                        .update("Counters", up_a, mod_a)
                        .await
                        .expect("update a");

                    let mut up_b = Document::from_json_str(r#"{"count": 25}"#).unwrap();
                    up_b.set_id(document::DocID::from_string(&doc_b).unwrap());
                    up_b.set_counter_delta("count".to_string(), document::NormalValue::Int(5));
                    let mut mod_b = rapidhash::RapidHashSet::new();
                    mod_b.insert("count".to_string());
                    mutator
                        .update("Counters", up_b, mod_b)
                        .await
                        .expect("update b");
                })
            }
        })
        .await;
        registry.commit(&handle).await.expect("commit updates");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_a, "count").await,
            11,
            "doc A must reflect +1"
        );
        assert_eq!(
            read_counter_store(&db, "Counters", &doc_b, "count").await,
            25,
            "doc B must reflect +5"
        );
    })
    .unwrap()
}

/// A counter's committed value, or `None` when the document or the counter's
/// part is absent.
async fn read_counter_store_opt(
    db: &Arc<DB<RegolithStore>>,
    collection_name: &str,
    doc_id: &str,
    field: &str,
) -> Option<i64> {
    read_committed_doc(db, collection_name, doc_id)
        .await?
        .get(field)
        .and_then(NormalValue::as_int)
}

/// Read a doc from the committed store via a fresh read txn; `None` if absent.
async fn read_committed_doc(
    db: &Arc<DB<RegolithStore>>,
    collection_name: &str,
    doc_id: &str,
) -> Option<Document> {
    let collection = db
        .get_collection(collection_name)
        .expect("get collection")
        .expect("collection exists");
    let txn = db.new_txn(true).await.expect("read txn");
    let datastore = txn.datastore().expect("datastore");
    let systemstore = txn.systemstore().expect("systemstore");
    let doc_id_typed = document::DocID::from_string(doc_id).expect("doc id");
    collection
        .get_by_doc_id(&datastore, &systemstore, &doc_id_typed)
        .await
        .expect("get doc")
}

/// PCounter create-then-update in ONE registry txn: create at 5 then +3 in the
/// SAME txn, commit. The increment lands on the value the create staged: 8,
/// not 11 from a double-apply.
#[test]
fn explicit_txn_pcounter_create_then_update_same_txn() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(pcounter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");

                let created = mutator
                    .create(
                        "PCounters",
                        Document::from_json_str(r#"{"count": 5}"#).unwrap(),
                    )
                    .await
                    .expect("create");
                let doc_id = created.doc_id.to_string();

                let mut update_doc = Document::from_json_str(r#"{"count": 8}"#).unwrap();
                update_doc.set_id(document::DocID::from_string(&doc_id).unwrap());
                update_doc.set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                let mut modified = rapidhash::RapidHashSet::new();
                modified.insert("count".to_string());
                mutator
                    .update("PCounters", update_doc, modified)
                    .await
                    .expect("update");
                doc_id
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "PCounters", &doc_id, "count").await,
            8,
            "PCounter create(5)+update(+3) in one txn must commit exactly 8"
        );
    })
    .unwrap()
}

/// A document written straight through `create_with_indexes`, as a legacy or
/// migrated document is, holds its counter only in the document value. An
/// interactive UPDATE +3 lands on that value: 5 + 3 = 8, never 0 + 3.
#[test]
fn an_increment_lands_on_a_directly_written_counter_value() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");

        // SETUP: write a doc with `count = 5` directly via create_with_indexes and
        // commit it with force_commit, outside any registry transaction.
        let collection = db
            .get_collection("Counters")
            .expect("get collection")
            .expect("collection exists");
        let index_manager =
            IndexManager::from_collection(collection.resolved_root_id(), collection.schema())
                .expect("index manager");

        let mut setup_doc = Document::from_json_str(r#"{"count": 5}"#).expect("doc");
        setup_doc.set_id(document::DocID::new_v0_from_seed("legacy-counter-doc"));
        let doc_id = setup_doc.id().expect("doc id").to_string();

        let setup_txn = db.new_txn(false).await.expect("write txn");
        let datastore = setup_txn.datastore().expect("datastore");
        let systemstore = setup_txn.systemstore().expect("systemstore");
        let doc_short_id = db::docid::map::next_doc_short_id(&systemstore)
            .await
            .expect("short id");
        db::docid::map::set_doc_id_mapping(
            &systemstore,
            collection.resolved_root_id(),
            doc_short_id,
            &doc_id,
        )
        .await
        .expect("doc id mapping");
        collection
            .create_with_indexes(&datastore, &setup_doc, doc_short_id, &index_manager)
            .await
            .expect("direct create");
        drop(datastore);
        drop(systemstore);
        setup_txn.force_commit().await.expect("force commit setup");

        let committed = read_committed_doc(&db, "Counters", &doc_id)
            .await
            .expect("committed doc present after setup");
        assert_eq!(
            committed.get("count"),
            Some(&document::NormalValue::Int(5)),
            "setup blob must hold the committed counter value 5"
        );

        // Interactive UPDATE +3, commit via the registry.
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin(false).await.expect("begin");
        in_txn(&registry, &handle, {
            let doc_id = doc_id.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");
                    let mut update_doc = Document::from_json_str(r#"{"count": 8}"#).expect("doc");
                    update_doc.set_id(document::DocID::from_string(&doc_id).expect("doc id"));
                    update_doc
                        .set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                    let mut modified = rapidhash::RapidHashSet::new();
                    modified.insert("count".to_string());
                    mutator
                        .update("Counters", update_doc, modified)
                        .await
                        .expect("update");
                })
            }
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            8,
            "the increment lands on the stored 5: 8, not 3"
        );
    })
    .unwrap()
}

/// PNCounter create-then-update in ONE registry txn: +3 then -5 in one txn
/// (create at 3, then decrement by 5). The signed result is -2 (3 + (-5)): the
/// decrement lands on the value the create staged in the same txn.
#[test]
fn explicit_txn_pncounter_create_then_decrement_same_txn() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");

                let created = mutator
                    .create(
                        "Counters",
                        Document::from_json_str(r#"{"count": 3}"#).unwrap(),
                    )
                    .await
                    .expect("create");
                let doc_id = created.doc_id.to_string();

                let mut update_doc = Document::from_json_str(r#"{"count": -2}"#).unwrap();
                update_doc.set_id(document::DocID::from_string(&doc_id).unwrap());
                update_doc.set_counter_delta("count".to_string(), document::NormalValue::Int(-5));
                let mut modified = rapidhash::RapidHashSet::new();
                modified.insert("count".to_string());
                mutator
                    .update("Counters", update_doc, modified)
                    .await
                    .expect("update");
                doc_id
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            -2,
            "PNCounter create(+3)+decrement(-5) in one txn must commit exactly -2"
        );
    })
    .unwrap()
}

/// Discard with pending counter ops: create a counter doc and increment it in
/// one registry txn, then roll back. The counter must have NO value and the doc
/// must be absent: discard drops the staged operands.
#[test]
fn explicit_txn_discard_drops_pending_counter_ops() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");

                let created = mutator
                    .create(
                        "Counters",
                        Document::from_json_str(r#"{"count": 7}"#).unwrap(),
                    )
                    .await
                    .expect("create");
                let doc_id = created.doc_id.to_string();

                let mut update_doc = Document::from_json_str(r#"{"count": 9}"#).unwrap();
                update_doc.set_id(document::DocID::from_string(&doc_id).unwrap());
                update_doc.set_counter_delta("count".to_string(), document::NormalValue::Int(2));
                let mut modified = rapidhash::RapidHashSet::new();
                modified.insert("count".to_string());
                mutator
                    .update("Counters", update_doc, modified)
                    .await
                    .expect("update");
                doc_id
            })
        })
        .await;
        registry.rollback(&handle).await.expect("rollback");

        assert_eq!(
            read_counter_store_opt(&db, "Counters", &doc_id, "count").await,
            None,
            "discard must leave NO counter value"
        );
        assert!(
            read_committed_doc(&db, "Counters", &doc_id).await.is_none(),
            "discard must leave the doc absent in the committed store"
        );
    })
    .unwrap()
}

/// Multiple updates to the SAME counter field in ONE registry txn: create at 0,
/// then +3 then +2 in the same txn. The two operands land on the SAME part,
/// summing to exactly 5 (each delta applied once).
#[test]
fn explicit_txn_multiple_updates_same_field_sum_once() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(counter_collection())
            .await
            .expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        // Seed a doc at 0 in its own committed txn.
        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");
                let created = mutator
                    .create(
                        "Counters",
                        Document::from_json_str(r#"{"count": 0}"#).unwrap(),
                    )
                    .await
                    .expect("create");
                created.doc_id.to_string()
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit create");

        // Two updates to the same field in ONE txn: +3 then +2.
        let handle = registry.begin(false).await.expect("begin");
        let visible = in_txn(&registry, &handle, {
            let doc_id = doc_id.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");

                    let mut up1 = Document::from_json_str(r#"{"count": 3}"#).unwrap();
                    up1.set_id(document::DocID::from_string(&doc_id).unwrap());
                    up1.set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                    let mut m1 = rapidhash::RapidHashSet::new();
                    m1.insert("count".to_string());
                    mutator.update("Counters", up1, m1).await.expect("update 1");

                    let mut up2 = Document::from_json_str(r#"{"count": 5}"#).unwrap();
                    up2.set_id(document::DocID::from_string(&doc_id).unwrap());
                    up2.set_counter_delta("count".to_string(), document::NormalValue::Int(2));
                    let mut m2 = rapidhash::RapidHashSet::new();
                    m2.insert("count".to_string());
                    mutator.update("Counters", up2, m2).await.expect("update 2");

                    ctx.doc_fetcher()
                        .get_by_ids("Counters", std::slice::from_ref(&doc_id))
                        .await
                        .unwrap()
                        .into_docs()
                })
            }
        })
        .await;
        assert_eq!(
            visible[0].get("count"),
            Some(&document::NormalValue::Int(5))
        );
        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            0,
            "uncommitted operands stay inside their transaction"
        );

        registry.commit(&handle).await.expect("commit updates");

        assert_eq!(
            read_counter_store(&db, "Counters", &doc_id, "count").await,
            5,
            "two same-field updates (+3,+2) in one txn must sum to exactly 5"
        );
    })
    .unwrap()
}

/// Counter collection with an @index on the counter field, exercising
/// `update_with_indexes` index maintenance for an increment.
fn indexed_counter_collection() -> CollectionVersion {
    let mut col = CollectionVersion::new(
        "IdxCounters",
        "icv1",
        "col-idx-counters",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "count", FieldKind::int()).with_crdt_type(CType::PnCounter),
        ],
    );
    col.indexes = vec![schema::IndexDescription::new("idx_count").with_field("count", false)];
    col
}

/// Indexed counter: increment in an interactive txn, commit, then assert the
/// index entry materialized at the incremented value (8). The unit-test layer has
/// no GraphQL filter-query executor, so this asserts the index entry directly
/// (the value a filter query would resolve against).
#[test]
fn explicit_txn_indexed_counter_index_reflects_the_incremented_value() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        let col_version = indexed_counter_collection();
        db.create_collection(col_version).await.expect("schema");
        let registry = DbTransactionRegistry::new(Arc::clone(&db));

        // Create at 5, commit.
        let handle = registry.begin(false).await.expect("begin");
        let doc_id = in_txn(&registry, &handle, move |ctx| {
            Box::pin(async move {
                let mutator = ctx.doc_mutator().expect("mutator");
                let created = mutator
                    .create(
                        "IdxCounters",
                        Document::from_json_str(r#"{"count": 5}"#).unwrap(),
                    )
                    .await
                    .expect("create");
                created.doc_id.to_string()
            })
        })
        .await;
        registry.commit(&handle).await.expect("commit create");

        // Increment +3 in an interactive txn, commit.
        let handle = registry.begin(false).await.expect("begin");
        in_txn(&registry, &handle, {
            let doc_id = doc_id.clone();
            move |ctx| {
                Box::pin(async move {
                    let mutator = ctx.doc_mutator().expect("mutator");
                    let mut update_doc = Document::from_json_str(r#"{"count": 8}"#).unwrap();
                    update_doc.set_id(document::DocID::from_string(&doc_id).unwrap());
                    update_doc
                        .set_counter_delta("count".to_string(), document::NormalValue::Int(3));
                    let mut modified = rapidhash::RapidHashSet::new();
                    modified.insert("count".to_string());
                    mutator
                        .update("IdxCounters", update_doc, modified)
                        .await
                        .expect("update");
                })
            }
        })
        .await;
        registry.commit(&handle).await.expect("commit update");

        // Authoritative store advanced to 8.
        assert_eq!(
            read_counter_store(&db, "IdxCounters", &doc_id, "count").await,
            8,
            "indexed counter must reflect +3 → 8"
        );

        // The index entry must exist at the incremented value 8 (what a filter query
        // `count: {_eq: 8}` would resolve), and must NOT exist at the stale 5.
        let collection = db
            .get_collection("IdxCounters")
            .expect("get collection")
            .expect("collection exists");
        let manager =
            IndexManager::from_collection(collection.resolved_root_id(), collection.schema())
                .expect("index manager");
        let index = manager.get_index("idx_count").expect("idx_count present");

        let txn = db.new_txn(true).await.expect("read txn");
        let datastore = txn.datastore().expect("datastore");

        let mut iter_8 = index
            .get(&datastore, &[document::NormalValue::Int(8)])
            .await
            .expect("index get 8");
        let entries_8 = iter_8.collect_all().await.expect("collect 8");
        assert_eq!(
            entries_8.len(),
            1,
            "index must have exactly one entry at the incremented value 8"
        );

        let mut iter_5 = index
            .get(&datastore, &[document::NormalValue::Int(5)])
            .await
            .expect("index get 5");
        let entries_5 = iter_5.collect_all().await.expect("collect 5");
        assert!(
            entries_5.is_empty(),
            "index must NOT have a stale entry at the pre-update value 5"
        );
    })
    .unwrap()
}

#[test]
fn create_in_tx_publishes_event_on_commit() {
    workers::block_on(async {
        let (db, bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");

        let mut sub = bus.subscribe(&[EventName::Update]);

        let txn = db.new_txn(false).await.expect("new_txn");
        let mutator = DbDocMutator::new(Arc::clone(&db), txn);

        let doc = Document::from_json_str(r#"{"x": 1}"#).expect("doc");
        let result = mutator.create("TestDoc", doc).await.expect("create");

        // Before commit: no event should have fired
        assert!(
            sub.try_recv().is_err(),
            "no event should fire before commit"
        );

        let txn = mutator.take_txn().await.expect("take txn");
        txn.commit().await.expect("commit");

        // After commit: one Update event arrives
        let msg = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("event arrived within timeout")
            .expect("subscription not closed");

        let update = msg.as_update().expect("expected Update message");
        assert_eq!(update.doc_id, result.doc_id.to_string());
        assert_ne!(update.cid, cid::Cid::default(), "cid should be populated");
        assert!(
            !update.block.is_empty(),
            "block bytes should be populated (matches Go's sendUpdate)"
        );
    })
    .unwrap()
}

#[test]
fn auto_commit_create_registers_acp_before_publishing_update() {
    workers::block_on(async {
        let (db, bus) = make_test_db_with_bus().await;
        db.create_collection(
            test_collection().with_policy(PolicyDescription::new("policy1", "test-docs")),
        )
        .await
        .expect("schema");

        let acp: Arc<dyn DocumentACP> =
            Arc::new(LocalDocumentACP::new(Arc::new(MemoryAcpStore::new())));
        let mutator = db::AutoCommitMutator::new(db);
        // The database gates a non-transactional provider itself.
        mutator.set_document_acp(acp.clone());
        let mut sub = bus.subscribe(&[EventName::Update]);
        let owner = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let create = workers::spawn(None, "test", move || async move {
            defra_core::signing::scope_broadcast_creator_did(
                Some(owner.to_string()),
                mutator.create(
                    "TestDoc",
                    Document::from_json_str(r#"{"x": 1}"#).expect("doc"),
                ),
            )
            .await
        })
        .unwrap();

        let message = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("event arrived within timeout")
            .expect("subscription not closed");
        let doc_id = message.as_update().expect("update").doc_id.clone();
        assert!(acp
            .is_doc_registered("policy1", "test-docs", &doc_id)
            .await
            .expect("registration check at event delivery"));
        let created = create.await.expect("create task").expect("create");
        assert_eq!(doc_id, created.doc_id.to_string());
        assert!(acp
            .is_doc_registered("policy1", "test-docs", &created.doc_id.to_string())
            .await
            .expect("registration check"));
    })
    .unwrap()
}

#[test]
fn create_in_tx_publishes_no_event_on_discard() {
    workers::block_on(async {
        let (db, bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");

        let mut sub = bus.subscribe(&[EventName::Update]);

        let txn = db.new_txn(false).await.expect("new_txn");
        let mutator = DbDocMutator::new(Arc::clone(&db), txn);

        let doc = Document::from_json_str(r#"{"x": 1}"#).expect("doc");
        mutator.create("TestDoc", doc).await.expect("create");

        let txn = mutator.take_txn().await.expect("take txn");
        txn.discard().expect("discard");

        // Allow a brief window for any (unexpected) async delivery
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(sub.try_recv().is_err(), "discard should not publish events");
    })
    .unwrap()
}

#[test]
fn delete_missing_doc_publishes_no_event_and_writes_no_block() {
    workers::block_on(async {
        // DeleteNode treats existed==false as a no-op; the mutator must not
        // create a tombstone commit or fire an Update event for a missing doc.
        let (db, bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");

        let mut sub = bus.subscribe(&[EventName::Update]);

        let txn = db.new_txn(false).await.expect("new_txn");
        let mutator = DbDocMutator::new(Arc::clone(&db), txn);

        let missing_doc_id = document::DocID::new_v0_from_seed("missing-doc");

        let result = mutator
            .delete("TestDoc", &missing_doc_id)
            .await
            .expect("delete should succeed even on missing doc");
        assert!(!result.existed, "doc should not have existed");

        let txn = mutator.take_txn().await.expect("take txn");
        txn.commit().await.expect("commit");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            sub.try_recv().is_err(),
            "deleting a non-existent doc should not publish an Update event"
        );
    })
    .unwrap()
}

/// `TxnBroadcaster` test double: captures every event it's asked to
/// broadcast for inspection.
struct CapturingBroadcaster {
    events: Arc<SegQueue<db::event::emission::TxnBroadcastEvent>>,
}

impl db::event::emission::TxnBroadcaster for CapturingBroadcaster {
    fn submit_update(
        &self,
        event: db::event::emission::TxnBroadcastEvent,
        _done: db::event::emission::BroadcastDone,
    ) {
        self.events.push(event);
    }
}

/// Records the composites local commits release, in the order released.
struct CapturingRelease {
    released: Arc<SegQueue<cid::Cid>>,
}

impl db::merge::governance::LocalCommitRelease for CapturingRelease {
    fn committed(&self, cid: cid::Cid, _block_data: bytes::Bytes) {
        self.released.push(cid);
    }
}

/// Two transactions commit one after the other; the first one's later
/// success work is held at a gate. Its broadcast and its release still go
/// out first, because they follow commit receipts, not the order the
/// transactions' tasks happen to reach them.
#[test]
fn transactional_broadcasts_and_releases_follow_commit_receipts() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");
        let captured: Arc<SegQueue<db::event::emission::TxnBroadcastEvent>> =
            Arc::new(SegQueue::new());
        let broadcaster: Arc<dyn db::event::emission::TxnBroadcaster> =
            Arc::new(CapturingBroadcaster {
                events: Arc::clone(&captured),
            });
        let released = Arc::new(SegQueue::new());
        db.set_local_commit_release(Arc::new(CapturingRelease {
            released: Arc::clone(&released),
        }));

        let entered = Arc::new(regolith::sync::Event::new());
        let gate = Arc::new(regolith::sync::Event::new());
        let first_txn = db.new_txn(false).await.expect("first txn");
        let (held_entered, held_gate) = (Arc::clone(&entered), Arc::clone(&gate));
        first_txn
            .on_commit(OnCommit::Async(Box::new(move || {
                Box::pin(async move {
                    held_entered.set();
                    held_gate.wait().await;
                })
            })))
            .expect("gate");
        let first = DbDocMutator::from_shared_txn_with_broadcaster(
            Arc::clone(&db),
            Arc::new(first_txn),
            Some(Arc::clone(&broadcaster)),
        );
        let first_doc = first
            .create(
                "TestDoc",
                Document::from_json_str(r#"{"x": 1}"#).expect("doc"),
            )
            .await
            .expect("first create");
        let first_txn = first.take_txn().await.expect("take first");
        let first_commit = workers::spawn(
            None,
            "test",
            move || async move { first_txn.commit().await },
        )
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), entered.wait())
            .await
            .expect("the first commit reached its success work");

        let second = DbDocMutator::from_shared_txn_with_broadcaster(
            Arc::clone(&db),
            Arc::new(db.new_txn(false).await.expect("second txn")),
            Some(broadcaster),
        );
        let second_doc = second
            .create(
                "TestDoc",
                Document::from_json_str(r#"{"x": 2}"#).expect("doc"),
            )
            .await
            .expect("second create");
        second
            .take_txn()
            .await
            .expect("take second")
            .commit()
            .await
            .expect("second commit");

        let events = drain(&captured);
        let broadcast: Vec<String> = events.iter().map(|event| event.doc_id.clone()).collect();
        assert_eq!(
            broadcast,
            vec![first_doc.doc_id.to_string(), second_doc.doc_id.to_string()]
        );
        let release_order: Vec<cid::Cid> = std::iter::from_fn(|| released.pop()).collect();
        let broadcast_cids: Vec<cid::Cid> = events.iter().map(|event| event.doc_cid).collect();
        assert_eq!(release_order, broadcast_cids);
        gate.set();
        first_commit.await.expect("join").expect("first commit");
    })
    .unwrap()
}

fn drain(
    events: &SegQueue<db::event::emission::TxnBroadcastEvent>,
) -> Vec<db::event::emission::TxnBroadcastEvent> {
    let mut drained = Vec::with_capacity(events.len());
    while let Some(event) = events.pop() {
        drained.push(event);
    }
    drained
}

#[test]
fn create_in_tx_forwards_to_broadcaster_on_commit() {
    workers::block_on(async {
        // F1 regression: a tx with a TxnBroadcaster wired in must invoke
        // broadcast_update for each committed mutation so P2P peers see
        // transactional writes (Go: db.sendUpdate → p2p.SendUpdate).
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");

        let captured: Arc<SegQueue<db::event::emission::TxnBroadcastEvent>> =
            Arc::new(SegQueue::new());
        let broadcaster: Arc<dyn db::event::emission::TxnBroadcaster> =
            Arc::new(CapturingBroadcaster {
                events: Arc::clone(&captured),
            });

        let txn = db.new_txn(false).await.expect("new_txn");
        let txn_arc = Arc::new(txn);
        let mutator = DbDocMutator::from_shared_txn_with_broadcaster(
            Arc::clone(&db),
            txn_arc,
            Some(broadcaster),
        );

        let doc = Document::from_json_str(r#"{"x": 1}"#).expect("doc");
        let result = mutator.create("TestDoc", doc).await.expect("create");

        // Broadcaster must NOT see anything before commit
        assert!(captured.is_empty(), "no broadcast before commit");

        let txn = mutator.take_txn().await.expect("take txn");
        txn.commit().await.expect("commit");

        // Wait briefly for the on_success_async callback to fire
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let events = drain(&captured);
        assert_eq!(events.len(), 1, "exactly one broadcast after commit");
        let event = &events[0];
        assert_eq!(event.doc_id, result.doc_id.to_string());
        assert_eq!(event.collection_name, "TestDoc");
        assert_ne!(event.doc_cid, cid::Cid::default(), "doc_cid populated");
        assert!(!event.doc_block.is_empty(), "doc_block populated");
        assert_eq!(
            event.document_json.as_ref().and_then(|json| json.get("x")),
            Some(&serde_json::json!(1)),
            "document_json populated for filtered transaction replication"
        );
    })
    .unwrap()
}

#[test]
fn create_in_tx_does_not_broadcast_on_discard() {
    workers::block_on(async {
        let (db, _bus) = make_test_db_with_bus().await;
        db.create_collection(test_collection())
            .await
            .expect("schema");

        let captured: Arc<SegQueue<db::event::emission::TxnBroadcastEvent>> =
            Arc::new(SegQueue::new());
        let broadcaster: Arc<dyn db::event::emission::TxnBroadcaster> =
            Arc::new(CapturingBroadcaster {
                events: Arc::clone(&captured),
            });

        let txn = db.new_txn(false).await.expect("new_txn");
        let txn_arc = Arc::new(txn);
        let mutator = DbDocMutator::from_shared_txn_with_broadcaster(
            Arc::clone(&db),
            txn_arc,
            Some(broadcaster),
        );

        let doc = Document::from_json_str(r#"{"x": 1}"#).expect("doc");
        mutator.create("TestDoc", doc).await.expect("create");

        let txn = mutator.take_txn().await.expect("take txn");
        txn.discard().expect("discard");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(captured.is_empty(), "discard should not trigger broadcast");
    })
    .unwrap()
}
