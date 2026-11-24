//! An index change in an explicit transaction runs as one statement: a
//! change that fails part way leaves none of its writes in the transaction.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use db::{DbTransactionRegistry, DB};
use defra_core::RequestContext;
use query::txn::TransactionRegistry;
use schema::{IndexKind, IndexedFieldDescription, OrderedIndexDescription};
use storage::corekv::Key;
use storage::keys::systemstore::CollectionNameKey;

use crate::common::failing_write_store::FailingWriteStore;

#[test]
fn a_create_that_fails_part_way_leaves_nothing_in_the_transaction() {
    workers::block_on(async {
        let armed = Arc::new(AtomicBool::new(false));
        let refusing = Arc::clone(&armed);
        let name_key = CollectionNameKey::new("User").bytes();
        // The definition is written first, then the name key: refusing the
        // name key fails the create after its first write.
        let store =
            FailingWriteStore::in_memory(Arc::new(move |key: &[u8], _value: Option<&[u8]>| {
                if refusing.load(Ordering::SeqCst) && key.strip_prefix(b"s") == Some(&name_key[..])
                {
                    return Err(storage::Error::Other("name key refused".into()));
                }
                Ok(())
            }));
        let db = Arc::new(DB::new(store).unwrap());
        let context = RequestContext::default();
        db.add_collection(&context, "type User { name: String }")
            .await
            .unwrap();
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin_as(&context, false).await.unwrap();

        armed.store(true, Ordering::SeqCst);
        let created = registry
            .create_index_in_txn(
                &context,
                handle.as_str(),
                "User",
                None,
                vec![IndexedFieldDescription {
                    name: "name".into(),
                    descending: false,
                }],
                IndexKind::Ordered(OrderedIndexDescription { unique: false }),
            )
            .await;
        armed.store(false, Ordering::SeqCst);
        assert!(created.is_err(), "the refused name key fails the create");

        registry.commit(&handle).await.unwrap();

        let indexed: Vec<_> = db
            .get_all_collection_versions()
            .await
            .unwrap()
            .into_iter()
            .filter(|version| !version.indexes.is_empty())
            .map(|version| version.version_id)
            .collect();
        assert!(
            indexed.is_empty(),
            "the failed create left an index definition in the transaction: {indexed:?}"
        );
        let cached = db.get_collection("User").unwrap().unwrap();
        assert!(cached.get_indexes().is_empty(), "nor in the cache");
    })
    .unwrap();
}
