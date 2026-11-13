//! AddView and RefreshViews in an explicit transaction: a failed AddView
//! leaves nothing behind, and a RefreshViews is refused in the transaction
//! where its autocommit form would refuse it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use db::{CollectionSelector, DbTransactionRegistry, DB};
use defra_core::RequestContext;
use query::txn::TransactionRegistry;
use storage::corekv::Key;
use storage::keys::systemstore::CollectionNameKey;
use storage::RegolithStore;

use crate::common::failing_write_store::FailingWriteStore;

#[test]
fn an_add_view_that_fails_part_way_leaves_nothing_in_the_transaction() {
    workers::block_on(async {
        let armed = Arc::new(AtomicBool::new(false));
        let refusing = Arc::clone(&armed);
        let second = CollectionNameKey::new("Second").bytes();
        // `First` is staged whole; `Second` fails at its name key.
        let store =
            FailingWriteStore::in_memory(Arc::new(move |key: &[u8], _value: Option<&[u8]>| {
                if refusing.load(Ordering::SeqCst) && key.strip_prefix(b"s") == Some(&second[..]) {
                    return Err(storage::Error::Other("name key refused".into()));
                }
                Ok(())
            }));
        let db = Arc::new(DB::new(store).unwrap());
        let context = RequestContext::default();
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin_as(&context, false).await.unwrap();
        let collections = query::parse_sdl_with_known_types(
            "type First { name: String } type Second { name: String }",
            Default::default(),
        )
        .unwrap();

        armed.store(true, Ordering::SeqCst);
        let added = registry
            .add_view_in_txn(
                &context,
                handle.as_str(),
                collections,
                None,
                None,
                Vec::new(),
                Vec::new(),
            )
            .await;
        armed.store(false, Ordering::SeqCst);
        assert!(added.is_err(), "the refused name key fails the add");

        registry.commit(&handle).await.unwrap();
        let left: Vec<_> = db
            .get_all_collection_versions()
            .await
            .unwrap()
            .into_iter()
            .map(|version| version.name)
            .collect();
        assert!(
            left.is_empty(),
            "the failed add left collections in the transaction: {left:?}"
        );
    })
    .unwrap();
}

#[test]
fn a_refresh_of_an_unknown_collection_fails_in_the_transaction() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let context = RequestContext::default();
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let selector = CollectionSelector {
            collection_id: Some("bafy-unknown".into()),
            ..Default::default()
        };
        let outside = db.refresh_views(selector.clone()).await;
        assert!(outside.is_err(), "outside a transaction it fails");

        let handle = registry.begin_as(&context, false).await.unwrap();
        let inside = registry
            .refresh_views_in_txn(&context, handle.as_str(), selector)
            .await;
        assert_eq!(
            inside.map_err(|error| error.to_string()),
            outside.map_err(|error| error.to_string()),
            "in a transaction it fails the same way, at the request"
        );
    })
    .unwrap();
}

#[test]
fn a_refresh_in_a_read_only_transaction_is_refused() {
    workers::block_on(async {
        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let context = RequestContext::default();
        let registry = DbTransactionRegistry::new(Arc::clone(&db));
        let handle = registry.begin_as(&context, true).await.unwrap();
        let refreshed = registry
            .refresh_views_in_txn(&context, handle.as_str(), CollectionSelector::all())
            .await;
        assert!(
            refreshed.is_err(),
            "a read-only transaction may not ask for the rebuild's writes"
        );
    })
    .unwrap();
}
