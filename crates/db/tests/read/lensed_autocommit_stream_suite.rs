use crate::common::fixture::fixture_with_docs;
use crate::common::stream::FailingCloseStream;
use crate::common::stream::RecordingStream;
use db::read::lensed::autocommit::stream::LensedAutoCommitDocStream;
use db::Collection;
use db::LensedAutoCommitFetcher;
use db::DB;
use query::doc_stream::DocStream;
use query::runner::DocFetcher;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use storage::RegolithStore;

fn wrapped_stream(
    db: Arc<DB<RegolithStore>>,
    collection: Collection,
    inner: Box<dyn DocStream>,
    closed: Arc<AtomicBool>,
) -> LensedAutoCommitDocStream<RegolithStore> {
    LensedAutoCommitDocStream::new(
        Box::new(RecordingStream { inner, closed }),
        None,
        LensedAutoCommitFetcher::new(db),
        collection,
        false,
        None,
    )
}

/// Draining a stream to exhaustion must close the storage iterator, not just
/// drop it: `release_read_txn` clears `inner`, so a later `ScanNode::close`
/// cannot reach it.
#[test]
fn exhaustion_closes_the_inner_stream() {
    workers::block_on(async {
        let db = fixture_with_docs(3).await;
        let collection = db.get_collection("Users").unwrap().unwrap();
        let closed = Arc::new(AtomicBool::new(false));

        let inner = LensedAutoCommitFetcher::new(db.clone())
            .stream_all_with_deleted("Users", false)
            .await
            .unwrap();
        let mut stream = wrapped_stream(db.clone(), collection, inner, closed.clone());

        while stream.next().await.unwrap().is_some() {}

        assert!(
            closed.load(Ordering::SeqCst),
            "exhaustion dropped the inner stream without closing it"
        );
    })
    .unwrap()
}

/// An explicit close before exhaustion must also reach it.
#[test]
fn explicit_close_closes_the_inner_stream() {
    workers::block_on(async {
        let db = fixture_with_docs(3).await;
        let collection = db.get_collection("Users").unwrap().unwrap();
        let closed = Arc::new(AtomicBool::new(false));

        let inner = LensedAutoCommitFetcher::new(db.clone())
            .stream_all_with_deleted("Users", false)
            .await
            .unwrap();
        let mut stream = wrapped_stream(db.clone(), collection, inner, closed.clone());

        stream.next().await.unwrap();
        stream.close().await.unwrap();

        assert!(closed.load(Ordering::SeqCst));
    })
    .unwrap()
}

#[test]
fn close_propagates_inner_error_and_is_idempotent() {
    workers::block_on(async {
        let db = fixture_with_docs(1).await;
        let collection = db.get_collection("Users").unwrap().unwrap();
        let mut stream = LensedAutoCommitDocStream::new(
            Box::new(FailingCloseStream),
            None,
            LensedAutoCommitFetcher::new(db),
            collection,
            false,
            None,
        );
        assert!(stream
            .close()
            .await
            .unwrap_err()
            .to_string()
            .contains("boom-close"));
        stream.close().await.unwrap();
    })
    .unwrap()
}
