//! A transaction shared by its namespace views holds no lock: views write
//! side by side, callbacks registered through any handle keep their order
//! and their savepoint, and a savepoint or a commit with a view still open is
//! refused at once rather than waited on.

use bytes::Bytes;
use datastore::{BasicTxn, Error, OnAbort, OnCommit};
use regolith::sync::SegQueue;
use std::sync::Arc;
use storage::corekv::{Reader, Store};
use storage::RegolithStore;

/// What the callbacks of a test saw, in the order they ran.
#[derive(Clone, Default)]
struct Seen(Arc<SegQueue<&'static str>>);

impl Seen {
    fn commit(&self, name: &'static str) -> OnCommit {
        let seen = self.clone();
        OnCommit::Sync(Box::new(move || seen.0.push(name)))
    }

    fn abort(&self, name: &'static str) -> OnAbort {
        let seen = self.clone();
        OnAbort::Sync(Box::new(move || seen.0.push(name)))
    }

    fn take(&self) -> Vec<&'static str> {
        std::iter::from_fn(|| self.0.pop()).collect()
    }
}

#[test]
fn views_write_side_by_side_and_the_commit_keeps_every_write() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let txn = BasicTxn::new(&store, 1, false).await.unwrap();
        let (data, system) = (txn.datastore(), txn.systemstore());
        let writes = (0..32u8).map(|i| {
            let (data, system) = (data.clone(), system.clone());
            async move {
                data.set(&[b'd', i], &[i]).await.unwrap();
                system.set(&[b's', i], &[i]).await.unwrap();
            }
        });
        futures::future::join_all(writes).await;
        drop((data, system));
        txn.commit().await.unwrap();

        let reader = store.new_txn(true).await.unwrap();
        for i in 0..32u8 {
            assert_eq!(
                reader.get(&[b'd', b'd', i]).await.unwrap(),
                Some(Bytes::from(vec![i]))
            );
            assert_eq!(
                reader.get(&[b's', b's', i]).await.unwrap(),
                Some(Bytes::from(vec![i]))
            );
        }
    })
    .unwrap()
}

#[test]
fn a_savepoint_with_a_view_open_is_refused_at_once() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let mut txn = BasicTxn::new(&store, 1, false).await.unwrap();
        let view = txn.datastore();
        view.set(b"key", b"value").await.unwrap();
        assert!(matches!(txn.set_savepoint(), Err(Error::TxnStillInUse)));
        drop(view);
        txn.set_savepoint().unwrap();
        txn.release_savepoint().unwrap();
        txn.commit().await.unwrap();
    })
    .unwrap()
}

#[test]
fn callbacks_from_every_handle_keep_their_order_and_their_savepoint() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let seen = Seen::default();
        let mut txn = BasicTxn::new(&store, 1, false).await.unwrap();
        let view = txn.systemstore();
        view.set(b"key", b"value").await.unwrap();
        view.on_commit(seen.commit("view before")).await.unwrap();
        txn.on_commit_shared(seen.commit("lent before")).unwrap();
        drop(view);

        txn.set_savepoint().unwrap();
        let view = txn.systemstore();
        view.on_commit(seen.commit("view after")).await.unwrap();
        txn.on_commit_shared(seen.commit("lent after")).unwrap();
        drop(view);
        txn.rollback_to_savepoint().unwrap();

        txn.on_commit_shared(seen.commit("lent last")).unwrap();
        txn.commit().await.unwrap();
        assert_eq!(seen.take(), vec!["view before", "lent before", "lent last"]);
    })
    .unwrap()
}

#[test]
fn a_read_only_transaction_refuses_ordered_work_from_any_handle() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let txn = BasicTxn::new(&store, 1, true).await.unwrap();
        let ordered = || OnCommit::Ordered(Box::new(|| {}));
        assert!(txn.datastore().on_commit(ordered()).await.is_err());
        assert!(txn.on_commit_shared(ordered()).is_err());
        assert!(txn
            .on_commit_shared(OnCommit::Sync(Box::new(|| {})))
            .is_ok());
    })
    .unwrap()
}

#[test]
fn a_discard_runs_every_abort_callback_even_with_a_view_still_open() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().unwrap();
        let seen = Seen::default();
        let txn = BasicTxn::new(&store, 1, false).await.unwrap();
        let view = txn.datastore();
        view.set(b"key", b"never").await.unwrap();
        txn.on_abort_shared(seen.abort("lent"));
        txn.on_commit_shared(seen.commit("committed")).unwrap();
        assert!(matches!(txn.discard(), Err(Error::TxnStillInUse)));
        drop(view);
        assert_eq!(seen.take(), vec!["lent"]);

        let reader = store.new_txn(true).await.unwrap();
        assert_eq!(reader.get(b"dkey").await.unwrap(), None);
    })
    .unwrap()
}
