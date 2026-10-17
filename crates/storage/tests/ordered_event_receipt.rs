use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use storage::backends::regolith::RegolithStore;
use storage::corekv::{OnCommit, Reader, Store, Writer};

#[test]
fn eventful_empty_commit_commits_and_dispatches_its_events() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().expect("store");
        let mut first = store.new_txn(false).await.expect("first txn");
        first.set(b"key", b"value").await.expect("write");
        first.commit().await.expect("first commit");

        let published = Arc::new(AtomicUsize::new(0));
        let mut empty = store.new_txn(false).await.expect("empty txn");
        let published_by_callback = Arc::clone(&published);
        empty
            .on_commit(OnCommit::Ordered(Box::new(move || {
                published_by_callback.fetch_add(1, Ordering::SeqCst);
            })))
            .expect("register event");

        empty.commit().await.expect("a commit that writes nothing");
        assert_eq!(published.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

#[test]
fn accepted_identical_blind_write_keeps_its_event_callback() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().expect("store");
        let mut seed = store.new_txn(false).await.expect("seed txn");
        seed.set(b"key", b"initial").await.expect("seed write");
        seed.commit().await.expect("seed commit");

        let mut delayed = store.new_txn(false).await.expect("delayed txn");
        delayed.set(b"key", b"same").await.expect("delayed write");
        let published = Arc::new(AtomicUsize::new(0));
        let published_by_callback = Arc::clone(&published);
        delayed
            .on_commit(OnCommit::Ordered(Box::new(move || {
                published_by_callback.fetch_add(1, Ordering::SeqCst);
            })))
            .expect("register event");

        let mut concurrent = store.new_txn(false).await.expect("concurrent txn");
        concurrent
            .set(b"key", b"same")
            .await
            .expect("concurrent write");
        concurrent.commit().await.expect("concurrent commit");

        delayed
            .commit()
            .await
            .expect("identical blind write is accepted");
        assert_eq!(published.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

#[test]
fn failed_ordered_hook_keeps_the_commit_its_events_and_later_commits() {
    workers::block_on(async {
        let store = RegolithStore::in_memory().expect("store");
        let published = Arc::new(AtomicUsize::new(0));

        let mut first = store.new_txn(false).await.expect("first txn");
        first.set(b"first", b"value").await.expect("first write");
        let published_by_first = Arc::clone(&published);
        first
            .on_commit(OnCommit::Ordered(Box::new(move || {
                published_by_first.fetch_add(1, Ordering::SeqCst);
            })))
            .expect("first event");
        first
            .on_commit(OnCommit::OrderedAsync(Box::new(|| {
                Box::pin(async { Err("external postcommit hook failed".to_string()) })
            })))
            .expect("failing hook");
        first
            .commit()
            .await
            .expect("a durable commit stands when its hook fails");
        assert_eq!(published.load(Ordering::SeqCst), 1);

        let mut second = store.new_txn(false).await.expect("second txn");
        second.set(b"second", b"value").await.expect("second write");
        let published_by_second = Arc::clone(&published);
        second
            .on_commit(OnCommit::Ordered(Box::new(move || {
                published_by_second.fetch_add(1, Ordering::SeqCst);
            })))
            .expect("second event");
        second.commit().await.expect("second commit");
        assert_eq!(published.load(Ordering::SeqCst), 2);

        let reader = store.new_txn(true).await.expect("read txn");
        for key in [b"first".as_slice(), b"second"] {
            assert_eq!(
                reader.get(key).await.expect("read").as_deref(),
                Some(b"value".as_slice()),
                "both commits stand"
            );
        }
    })
    .unwrap()
}
