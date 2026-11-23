//! The two-second sweep's storage reads per peer are bounded by one page of
//! markers, never by the peer's marker count: a not-due peer costs the same
//! with one marker as with thousands, and a due peer reads one page.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use kovan::Atom;
use rapidhash::{HashMapExt, RapidHashMap};
use storage::backends::RegolithStore;
use storage::corekv::private::Sealed;
use storage::corekv::{IterOptions, Iterator, KvPair, Reader, Result, Store, Txn, Writer};
use storage::stores::{Peerstore, RetryInfo};

use super::sweep_tests::{ConnectedTransport, Outcome, ScriptedPusher, DOCUMENT_FAILURE};
use crate::retry::{run_retry_pass, MAX_MARKERS_PER_PEER_PASS};
use crate::transport_doc_pusher::TransportDocPusher;

const PEER: &str = "peer-a";

/// Counts the keys iterators yield through the sweep's own snapshots.
struct CountingStore {
    inner: RegolithStore,
    keys_read: Arc<AtomicUsize>,
}

impl Sealed for CountingStore {}

#[async_trait]
impl Store for CountingStore {
    fn crdts(&self) -> &storage::Crdts {
        self.inner.crdts()
    }

    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> Result<Box<dyn Txn>> {
        Ok(Box::new(CountingTxn {
            inner: self.inner.new_txn(readonly).await?,
            keys_read: Arc::clone(&self.keys_read),
        }))
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> std::result::Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError>
    {
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> Result<()> {
        self.inner.close().await
    }
}

struct CountingTxn {
    inner: Box<dyn Txn>,
    keys_read: Arc<AtomicUsize>,
}

impl Sealed for CountingTxn {}

#[async_trait]
impl Reader for CountingTxn {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn has(&self, key: &[u8]) -> Result<bool> {
        self.inner.has(key).await
    }

    async fn get_size(&self, key: &[u8]) -> Result<Option<usize>> {
        self.inner.get_size(key).await
    }

    async fn iterator(&self, opts: IterOptions) -> Result<Box<dyn Iterator>> {
        Ok(Box::new(CountingIterator {
            inner: self.inner.iterator(opts).await?,
            keys_read: Arc::clone(&self.keys_read),
        }))
    }
}

#[async_trait]
impl Writer for CountingTxn {
    async fn set_shared(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner.set_shared(key, value).await
    }

    async fn delete_shared(&self, key: &[u8]) -> Result<()> {
        self.inner.delete_shared(key).await
    }
}

#[async_trait]
impl Txn for CountingTxn {
    async fn commit(self: Box<Self>) -> Result<()> {
        self.inner.commit().await
    }

    fn discard(self: Box<Self>) {
        self.inner.discard()
    }

    fn on_commit(&mut self, callback: storage::corekv::OnCommit) -> Result<()> {
        self.inner.on_commit(callback)
    }

    fn on_abort(&mut self, callback: storage::corekv::OnAbort) {
        self.inner.on_abort(callback)
    }

    fn before_commit(&mut self, callback: storage::corekv::BeforeCommit) {
        self.inner.before_commit(callback)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn is_readonly(&self) -> bool {
        self.inner.is_readonly()
    }

    fn callback_count(&self) -> usize {
        self.inner.callback_count()
    }
}

struct CountingIterator {
    inner: Box<dyn Iterator>,
    keys_read: Arc<AtomicUsize>,
}

impl Sealed for CountingIterator {}

#[async_trait]
impl Iterator for CountingIterator {
    async fn next(&mut self) -> Result<Option<KvPair>> {
        let pulled = self.inner.next().await?;
        if pulled.is_some() {
            self.keys_read.fetch_add(1, Ordering::SeqCst);
        }
        Ok(pulled)
    }

    async fn close(&mut self) -> Result<()> {
        self.inner.close().await
    }

    async fn seek(&mut self, key: &[u8]) -> Result<bool> {
        self.inner.seek(key).await
    }

    async fn reset(&mut self) -> Result<()> {
        self.inner.reset().await
    }

    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

/// Keys one sweep pass read for a peer holding `markers` markers, each of
/// which the receiver refuses. `due` decides whether the peer's clock is up.
async fn keys_read_by_one_pass(markers: usize, due: bool) -> usize {
    let inner = RegolithStore::in_memory().unwrap();
    let keys_read = Arc::new(AtomicUsize::new(0));
    let peerstore = Peerstore::new(Arc::new(CountingStore {
        inner: inner.clone(),
        keys_read: Arc::clone(&keys_read),
    }));
    let replicator =
        p2p::ReplicatorInfo::from_raw(PEER.to_string(), vec!["collection-a".into()], Vec::new());
    peerstore
        .create_replicator(PEER, &replicator.to_bytes().unwrap())
        .await
        .unwrap();
    let initial = RetryInfo::new_initial().to_bytes().unwrap();
    let mut script = RapidHashMap::new();
    for index in 0..markers {
        let doc = format!("doc-{index:05}");
        peerstore
            .record_push_failure(PEER, &doc, "collection-a", &initial)
            .await
            .unwrap();
        script.insert(doc, Outcome::Failed(DOCUMENT_FAILURE));
    }
    if due {
        peerstore.activate_retry_peer(PEER).await.unwrap();
    } else {
        peerstore
            .record_retry_after(PEER, Duration::from_secs(600))
            .await
            .unwrap();
    }
    let pusher: Arc<dyn TransportDocPusher> = Arc::new(ScriptedPusher {
        peerstore: Peerstore::new(Arc::new(inner)),
        script,
        attempted: Atom::new(Vec::new()),
    });
    let transport = ConnectedTransport {
        local: p2p::transport::PeerId::new("local".to_string()),
        connected: vec![p2p::transport::PeerId::new(PEER.to_string())],
    };
    keys_read.store(0, Ordering::SeqCst);
    run_retry_pass(&peerstore, &transport, &pusher, None, false).await;
    keys_read.load(Ordering::SeqCst)
}

#[nuclei::test]
async fn sweep_reads_are_constant_when_peer_not_due() {
    let one = keys_read_by_one_pass(1, false).await;
    let many = keys_read_by_one_pass(2_000, false).await;
    assert_eq!(
        many, one,
        "a not-due peer with 2,000 markers cost {many} key reads, with one marker {one}"
    );
}

#[nuclei::test]
async fn a_due_pass_reads_one_page_of_markers() {
    let page = keys_read_by_one_pass(MAX_MARKERS_PER_PEER_PASS * 2, true).await;
    let many = keys_read_by_one_pass(2_000, true).await;
    assert_eq!(
        many, page,
        "a due peer with 2,000 markers cost {many} key reads, with two pages {page}"
    );
}
