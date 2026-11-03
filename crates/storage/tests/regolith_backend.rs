//! The shared backend conformance suite, run against regolith.
//!
//! These tests belonged to every backend the tree used to carry. regolith is
//! the only one now, so it is the only thing that can hold the contract they
//! describe: read-your-writes, iterator direction and bounds, seek, callback
//! ordering, and behaviour under concurrent transactions.

use std::sync::Arc;
use storage::RegolithStore;

async fn store() -> RegolithStore {
    RegolithStore::in_memory().expect("in-memory regolith store")
}

async fn arc_store() -> Arc<RegolithStore> {
    Arc::new(store().await)
}

storage::generate_backend_tests!(store);
storage::generate_backend_concurrency_tests!(arc_store);
storage::generate_backend_dropable_tests!(store);

/// Adding Defra's background-error listener must preserve caller listeners.
#[test]
fn caller_listener_receives_transaction_conflicts() {
    workers::block_on(async {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use storage::corekv::{Reader, Store, Writer};
        use storage::RegolithStoreOptions;

        #[derive(Default)]
        struct Conflicts(AtomicUsize);

        impl regolith::EventListener for Conflicts {
            fn on_conflict(&self, _: &regolith::Conflict) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let listener = Arc::new(Conflicts::default());
        let mut options = RegolithStoreOptions::memory();
        options.engine = options.engine.add_listener(listener.clone());
        let store = RegolithStore::open_with_options("listener-test", options).unwrap();

        let mut first = store.new_txn(false).await.unwrap();
        let mut second = store.new_txn(false).await.unwrap();
        assert!(first.get(b"claim").await.unwrap().is_none());
        assert!(second.get(b"claim").await.unwrap().is_none());
        first.set(b"claim", b"first").await.unwrap();
        second.set(b"claim", b"second").await.unwrap();
        first.commit().await.unwrap();
        let error = second.commit().await.unwrap_err();
        let storage::Error::TxnConflictDetails(conflict) = error else {
            panic!("Regolith validation conflicts should preserve safe reason metadata");
        };
        assert_eq!(conflict.access, regolith::Access::Read);
        assert_eq!(conflict.winning_write, regolith::WriteKind::Put);
        assert!(conflict.latest_seq > conflict.observed_seq);
        assert_eq!(listener.0.load(Ordering::Relaxed), 1);
    })
    .unwrap()
}
