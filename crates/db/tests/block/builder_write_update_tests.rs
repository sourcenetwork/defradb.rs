use datastore::NamespaceView;
use datastore::SharedTxn;
use db::block::builder::*;
use document::DocID;
use document::Document;
use document::NormalValue;
use rapidhash::RapidHashSet;
use storage::corekv::Store;
use storage::namespace::Namespace;
use storage::RegolithStore;

#[test]
fn update_keeps_doc_id_without_composite_heads() {
    workers::block_on(async {
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
        let mut removal = storage::merge::head_set::Write::default();
        removal.supersede(&db::block::heads::document::name("C", &created.cid));
        db::block::heads::document::commit(&headstore, identity.doc_short_id, &removal)
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
    })
    .unwrap()
}
