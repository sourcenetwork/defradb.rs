use super::*;

#[test]
fn a_named_transaction_never_falls_back_to_autocommit() {
    workers::block_on(async {
        let database = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
        let store = Arc::new(acp::MemoryZanzibarStore::new());
        let acp = Arc::new(acp::ZanzibarDocumentACP::new(store.clone()));
        let adapter = DocumentAcpAdapter::new(database, acp, store);
        let caller = identity::Did::new("did:key:caller").unwrap();
        let context = RequestContext::default()
            .with_identity(Some(caller.to_string()))
            .with_txn(Some(defra_core::TxnId(1)));
        let bound = adapter.bind_request(&context).unwrap();
        assert!(bound
            .add_doc_relationship(Some(&caller), "did:key:reader", "Record", "doc", "reader")
            .await
            .is_err());
        assert!(bound
            .delete_doc_relationship(Some(&caller), "did:key:reader", "Record", "doc", "reader")
            .await
            .is_err());
    })
    .unwrap();
}
