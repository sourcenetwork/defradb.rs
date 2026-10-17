use crate::corekv::{OnAbort, OnCommit, Store};
use futures::FutureExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Test discard prevents persistence
pub async fn test_discard_prevents_persistence<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();
    txn.set(b"discarded_key", b"value").await.unwrap();
    txn.discard();

    let txn = store.new_txn(true).await.unwrap();
    assert_eq!(
        txn.get(b"discarded_key").await.unwrap(),
        None,
        "Discarded transaction changes must not persist"
    );
}

/// Test success callback is invoked on commit
pub async fn test_success_callback<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    txn.on_commit(OnCommit::Sync(Box::new(move || {
        called_clone.store(true, Ordering::SeqCst);
    })))
    .unwrap();

    txn.set(b"key", b"value").await.unwrap();
    txn.commit().await.unwrap();

    assert!(
        called.load(Ordering::SeqCst),
        "Success callback should be invoked"
    );
}

/// Test discard callback is invoked
pub async fn test_discard_callback<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    txn.on_abort(OnAbort::Sync(Box::new(move || {
        called_clone.store(true, Ordering::SeqCst);
    })));

    txn.set(b"key", b"value").await.unwrap();
    txn.discard();

    assert!(
        called.load(Ordering::SeqCst),
        "Discard callback should be invoked"
    );
}

/// Test async success callback
pub async fn test_async_success_callback<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    txn.on_commit(OnCommit::Async(Box::new(move || {
        let flag = called_clone.clone();
        Box::pin(async move {
            workers::delay(std::time::Duration::from_millis(10))
                .unwrap()
                .await
                .unwrap();
            flag.store(true, Ordering::SeqCst);
        })
    })))
    .unwrap();

    txn.set(b"key", b"value").await.unwrap();
    txn.commit().await.unwrap();

    assert!(
        called.load(Ordering::SeqCst),
        "Async callback should be awaited during commit"
    );
}

/// Test that sync callback panic propagates to the caller.
pub async fn test_callback_panic_propagates<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let count = Arc::new(AtomicUsize::new(0));

    // First callback - increments
    let count1 = count.clone();
    txn.on_commit(OnCommit::Sync(Box::new(move || {
        count1.fetch_add(1, Ordering::SeqCst);
    })))
    .unwrap();

    // Second callback - PANICS
    txn.on_commit(OnCommit::Sync(Box::new(|| {
        panic!("Intentional panic in test");
    })))
    .unwrap();

    // Third callback - should not run once panic propagation begins
    let count3 = count.clone();
    txn.on_commit(OnCommit::Sync(Box::new(move || {
        count3.fetch_add(1, Ordering::SeqCst);
    })))
    .unwrap();

    txn.set(b"key", b"value").await.unwrap();
    let panic = std::panic::AssertUnwindSafe(async move {
        txn.commit().await.unwrap();
    })
    .catch_unwind()
    .await;

    assert!(
        panic.is_err(),
        "Callback panic should propagate from commit"
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "Callbacks after the panic should not execute"
    );
}

/// Test that async callback panic propagates to the caller.
pub async fn test_async_callback_panic_propagates<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let count = Arc::new(AtomicUsize::new(0));

    let count1 = count.clone();
    txn.on_commit(OnCommit::Async(Box::new(move || {
        let c = count1.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
        })
    })))
    .unwrap();

    txn.on_commit(OnCommit::Async(Box::new(|| {
        Box::pin(async {
            panic!("Intentional async panic");
        })
    })))
    .unwrap();

    let count3 = count.clone();
    txn.on_commit(OnCommit::Async(Box::new(move || {
        let c = count3.clone();
        Box::pin(async move {
            c.fetch_add(1, Ordering::SeqCst);
        })
    })))
    .unwrap();

    txn.set(b"key", b"value").await.unwrap();
    let panic = std::panic::AssertUnwindSafe(async move {
        txn.commit().await.unwrap();
    })
    .catch_unwind()
    .await;

    assert!(
        panic.is_err(),
        "Async callback panic should propagate from commit"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// Test that discard callback panic propagates to the caller.
pub async fn test_discard_callback_panic_propagates<S: Store>(store: &S) {
    let mut txn = store.new_txn(false).await.unwrap();

    let count = Arc::new(AtomicUsize::new(0));

    let count1 = count.clone();
    txn.on_abort(OnAbort::Sync(Box::new(move || {
        count1.fetch_add(1, Ordering::SeqCst);
    })));

    txn.on_abort(OnAbort::Sync(Box::new(|| {
        panic!("Discard callback panic");
    })));

    let count3 = count.clone();
    txn.on_abort(OnAbort::Sync(Box::new(move || {
        count3.fetch_add(1, Ordering::SeqCst);
    })));

    txn.set(b"key", b"value").await.unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        txn.discard();
    }));

    assert!(panic.is_err(), "Discard callback panic should propagate");
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
