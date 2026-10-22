use cid::Cid;
use datastore::NamespaceView;
use datastore::SharedTxn;
use db::block::builder::*;
use document::CType;
use document::Document;
use document::NormalValue;
use rapidhash::RapidHashSet;
use storage::corekv::Store;
use storage::namespace::Namespace;
use storage::RegolithStore;

async fn first_counter_update() -> Cid {
    let store = RegolithStore::in_memory().unwrap();
    let txn = store.new_txn(false).await.unwrap();
    let shared = SharedTxn::new(txn);
    let blockstore = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let headstore = NamespaceView::new(shared, Namespace::Headstore);
    let identity = DocRef::new(1, 1);

    let mut doc = Document::new(schema::CollectionVersion::new(
        "Test",
        "schema-v1",
        "col-test",
        vec![
            schema::FieldDescription::new(
                "1",
                "name",
                schema::FieldKind::string(),
                CType::LwwRegister,
            ),
            schema::FieldDescription::new("2", "count", schema::FieldKind::int(), CType::PnCounter),
        ],
    ));
    doc.set("name", NormalValue::String("Alice".to_string()))
        .unwrap();
    let created = write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &crate::common::schema::version("schema-v1"),
        identity,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    doc.set_id(document::DocID::from_string(&created.doc_id).unwrap());
    doc.set("count", NormalValue::Int(1)).unwrap();
    doc.edit(
        "count".to_string(),
        document::FieldOp::Increment(NormalValue::Int(1)),
    )
    .unwrap();
    let modified = RapidHashSet::from_iter(["count".to_string()]);

    write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &crate::common::schema::version("schema-v1"),
        identity,
        Some(&modified),
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .field_cids[0]
}

#[test]
fn first_counter_updates_on_existing_document_have_distinct_cids() {
    workers::block_on(async {
        let (left, right) = futures::join!(first_counter_update(), first_counter_update());

        assert_ne!(left, right);
    })
    .unwrap()
}
