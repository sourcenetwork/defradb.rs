//! An unfiltered truncate finishes while writers keep updating the documents
//! it removes (proofs/tla/TruncateUnderWriters.tla).

use crate::common::failing_write_store::FailingWriteStore;
use crate::common::schema::parsed;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cid::Cid;
use db::{AutoCommitFetcher, AutoCommitMutator, DB};
use defra_core::RequestContext;
use document::{DocID, Document, NormalValue};
use query::{DocFetcher, DocMutator};
use storage::corekv::{RetryAttemptFuture, Store};
use storage::RegolithStore;

const SDL: &str = "type U { n: Int @crdt(type: pcounter) @index g: Int }";

type Writer = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Commits `write` inside every attempt, after it ran and before it commits,
/// so each attempt meets a writer that committed after its snapshot.
struct RacedStore {
    inner: Arc<RegolithStore>,
    write: Writer,
    attempts: Arc<AtomicUsize>,
}

impl storage::corekv::private::Sealed for RacedStore {}

#[async_trait::async_trait]
impl storage::corekv::Store for RacedStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(
        &self,
        key: &[u8],
        count: u64,
    ) -> storage::corekv::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(
        &self,
        readonly: bool,
    ) -> storage::corekv::Result<Box<dyn storage::corekv::Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        let attempt = Box::new(RacedAttempt {
            inner: attempt,
            write: Arc::clone(&self.write),
            attempts: Arc::clone(&self.attempts),
        });
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> storage::corekv::Result<()> {
        self.inner.close().await
    }
}

struct RacedAttempt {
    inner: storage::corekv::RetryAttempt,
    write: Writer,
    attempts: Arc<AtomicUsize>,
}

impl storage::corekv::RetryAttemptFn for RacedAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn storage::corekv::RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let output = self.inner.run(txn).await?;
            (self.write)().await;
            Ok(output)
        })
    }
}

/// An increment of `n` and a new `g` for every document still there; a
/// document the truncate already took is not found, which a racing writer may
/// lose.
async fn update_all(db: &Arc<DB<RegolithStore>>, docs: &[DocID], round: i64) -> usize {
    let mut written = 0;
    for doc_id in docs {
        let mut doc = Document::new(parsed(SDL));
        doc.set_id(doc_id.clone());
        doc.set("n", NormalValue::Int(1)).unwrap();
        doc.set("g", NormalValue::Int(round)).unwrap();
        let fields = ["n".to_string(), "g".to_string()].into_iter().collect();
        if AutoCommitMutator::new(Arc::clone(db))
            .update("U", doc, fields)
            .await
            .is_ok()
        {
            written += 1;
        }
    }
    written
}

async fn document_heads<S>(db: &Arc<DB<S>>, docs: &[DocID]) -> Vec<(u64, Vec<Cid>)>
where
    S: Store + 'static,
{
    let txn = db.new_txn(true).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let headstore = txn.headstore().unwrap();
    let mut found = Vec::with_capacity(docs.len());
    for doc_id in docs {
        let doc_ref = db::docid::map::get_doc_ref(&systemstore, &doc_id.to_string())
            .await
            .unwrap()
            .expect("document is stored");
        let heads = db::block::heads::document::load(&headstore, doc_ref.doc_short_id)
            .await
            .unwrap();
        found.push((
            doc_ref.doc_short_id,
            db::block::heads::document::live_cids(&heads),
        ));
    }
    drop(headstore);
    drop(systemstore);
    let _ = txn.discard();
    found
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate_under_writers")]
fn an_unfiltered_truncate_finishes_while_every_attempt_meets_a_writer() {
    workers::block_on(async {
        let inner = Arc::new(RegolithStore::in_memory().unwrap());
        let writer_db = Arc::new(DB::open((*inner).clone()).await.unwrap());
        writer_db
            .add_collection(&RequestContext::ambient(), SDL)
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(Arc::clone(&writer_db));
        let mut docs = Vec::new();
        for g in 0..8 {
            let mut doc = Document::new(parsed(SDL));
            doc.set("n", NormalValue::Int(0)).unwrap();
            doc.set("g", NormalValue::Int(g)).unwrap();
            docs.push(mutator.create("U", doc).await.unwrap().doc_id);
        }
        update_all(&writer_db, &docs, 100).await;
        let heads = document_heads(&writer_db, &docs).await;

        let round = Arc::new(AtomicUsize::new(0));
        let write: Writer = {
            let (writer_db, docs) = (Arc::clone(&writer_db), docs.clone());
            Arc::new(move || {
                let (writer_db, docs) = (Arc::clone(&writer_db), docs.clone());
                let round = round.fetch_add(1, Ordering::SeqCst) as i64;
                Box::pin(async move {
                    update_all(&writer_db, &docs, round).await;
                })
            })
        };
        let attempts = Arc::new(AtomicUsize::new(0));
        let store = RacedStore {
            inner: Arc::clone(&inner),
            write,
            attempts: Arc::clone(&attempts),
        };
        let db = Arc::new(DB::open(store).await.unwrap());

        db.truncate_collection_as(&RequestContext::ambient(), "U", None)
            .await
            .expect("a truncate whose every attempt meets a writer still finishes");

        let remaining = AutoCommitFetcher::new(Arc::clone(&writer_db))
            .get_all_with_deleted("U", true)
            .await
            .unwrap();
        assert!(
            remaining.is_empty(),
            "every document is gone: {remaining:?}"
        );
        assert_eq!(
            update_all(&writer_db, &docs, 0).await,
            0,
            "no document can be updated once truncated"
        );

        let txn = db.new_txn(true).await.unwrap();
        let headstore = txn.headstore().unwrap();
        for (doc_short_id, cids) in &heads {
            let remaining = db::block::heads::document::load(&headstore, *doc_short_id)
                .await
                .unwrap();
            assert!(
                db::block::heads::document::live_cids(&remaining).is_empty(),
                "the purge removes the document heads for {doc_short_id}"
            );
            let blockstore = txn.blockstore().unwrap();
            for cid in cids {
                assert!(
                    blockstore.get(&cid.to_bytes()).await.unwrap().is_none(),
                    "the purge removes block {cid}"
                );
            }
        }
        drop(headstore);
        let _ = txn.discard();
    })
    .unwrap()
}

#[test]
#[mantra_macros::req_test("defra.collection.truncate_under_writers")]
fn a_failed_purge_leaves_marked_documents_and_a_rerun_finishes() {
    workers::block_on(async {
        let fail_first_purge = Arc::new(AtomicBool::new(false));
        let armed = Arc::clone(&fail_first_purge);
        let store = FailingWriteStore::in_memory(Arc::new(move |key, value| {
            if key.first() == Some(&b'd') && value.is_none() && armed.swap(false, Ordering::SeqCst)
            {
                return Err(storage::Error::Other("injected purge failure".into()));
            }
            Ok(())
        }));
        let db = Arc::new(DB::open(store).await.unwrap());
        db.add_collection(&RequestContext::ambient(), SDL)
            .await
            .unwrap();

        let mut docs: Vec<DocID> = Vec::new();
        for g in 0..2 {
            let mut doc = Document::new(parsed(SDL));
            doc.set("n", NormalValue::Int(0)).unwrap();
            doc.set("g", NormalValue::Int(g)).unwrap();
            docs.push(
                AutoCommitMutator::new(Arc::clone(&db))
                    .create("U", doc)
                    .await
                    .unwrap()
                    .doc_id,
            );
        }

        fail_first_purge.store(true, Ordering::SeqCst);
        let error = db
            .truncate_collection_as(&RequestContext::ambient(), "U", None)
            .await
            .expect_err("the injected purge failure must reach the caller");
        assert!(
            error.to_string().contains("injected purge failure"),
            "{error}"
        );

        let marked = AutoCommitFetcher::new(Arc::clone(&db))
            .get_all_with_deleted("U", true)
            .await
            .unwrap();
        assert_eq!(
            marked.len(),
            docs.len(),
            "the marks remain after the failure"
        );
        for doc_id in &docs {
            let mut doc = Document::new(parsed(SDL));
            doc.set_id(doc_id.clone());
            doc.set("n", NormalValue::Int(1)).unwrap();
            let error = AutoCommitMutator::new(Arc::clone(&db))
                .update("U", doc, ["n".to_string()].into_iter().collect())
                .await
                .expect_err("a marked document cannot be updated");
            assert_eq!(error.to_string(), format!("document not found: {doc_id}"));
        }

        db.truncate_collection_as(&RequestContext::ambient(), "U", None)
            .await
            .expect("a second truncate finishes the purge");
        let remaining = AutoCommitFetcher::new(Arc::clone(&db))
            .get_all_with_deleted("U", true)
            .await
            .unwrap();
        assert!(
            remaining.is_empty(),
            "the rerun removes the marked documents"
        );
    })
    .unwrap();
}
