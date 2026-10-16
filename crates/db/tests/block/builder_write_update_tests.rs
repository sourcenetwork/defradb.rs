use datastore::NamespaceView;
use datastore::SharedTxn;
use db::block::builder::*;
use document::DocID;
use document::Document;
use document::NormalValue;
use rapidhash::RapidHashSet;
use storage::corekv::Key;
use storage::corekv::Store;
use storage::keys::headstore::HeadstoreDocKey;
use storage::namespace::Namespace;
use storage::RegolithStore;

#[tokio::test]
async fn update_keeps_doc_id_without_composite_heads() {
    let store = RegolithStore::in_memory().unwrap();
    let shared = SharedTxn::new(store.new_txn(false).await.unwrap());
    let blockstore = NamespaceView::new(shared.clone(), Namespace::Blockstore);
    let headstore = NamespaceView::new(shared, Namespace::Headstore);
    let identity = DocStorageIdentity::new(1, 1);

    let mut doc = Document::new();
    doc.set("name", NormalValue::String("Alice".to_string()));
    doc.set("age", NormalValue::Int(30));
    let created = write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &crate::common::schema::version("v1"),
        identity,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    headstore
        .delete(&HeadstoreDocKey::new(identity.doc_short_id, "C", created.cid).bytes())
        .await
        .unwrap();

    doc.set_id(DocID::from_string(&created.doc_id).unwrap());
    doc.set("name", NormalValue::String("Bob".to_string()));
    let changed = RapidHashSet::from_iter(["name".to_string()]);
    let updated = write_document_blocks(
        &blockstore,
        &headstore,
        &doc,
        &crate::common::schema::version("v1"),
        identity,
        Some(&changed),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    assert_eq!(updated.doc_id, created.doc_id);
}
