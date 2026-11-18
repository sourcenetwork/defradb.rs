//! A committed write keeps registering its sender markers after its caller
//! stops waiting, as a request past its deadline does.

use std::sync::Arc;
use std::time::Duration;

use blockstore::DefraBlockstore;
use db::merge::BroadcastMutator;
use db::DB;
use document::{DocID, Document, NormalValue};
use futures::future::{select, Either};
use p2p::sync::{SyncConfig, SyncCoordinator};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::Atom;
use storage::RegolithStore;

use super::push_docs_replay::ConnectedPeerTransport;

const SDL: &str = "type Article { body: String @chunk(maxChars: 64) }";
const CHUNKS: &str = "Article__body_chunk";
const PEER: &str = "receiver";
const FAILURE_CHANNEL_CAPACITY: usize = 64;
/// The generation the recorder gives every marker it registers.
const MARKER_GENERATION: u64 = 1;

/// Long enough for several chunks at the schema's `maxChars: 64`.
const BODY: &str = "First paragraph of the article, long enough for a chunk.\n\n\
    Second paragraph of the article, long enough for a chunk.\n\n\
    Third paragraph of the article, long enough for a chunk.\n\n\
    Fourth paragraph of the article, long enough for a chunk.";

/// How long the markers left after the caller stopped may take to arrive.
const HANDOFF_GRACE: Duration = Duration::from_secs(5);

/// Answers one coordinator event; true when it was a marker registration.
fn answer(pushed: &Atom<Vec<String>>, mut event: p2p::sync::PushFailure) -> bool {
    let Some(reply) = event.durable_tx.take() else {
        return false;
    };
    let registration = !event.admission_only && !event.acknowledged && !event.create_retry;
    if registration {
        pushed.rcu(|docs| {
            let mut next = docs.clone();
            next.push(event.doc_id.clone());
            next
        });
        let _ = reply.send(Some(MARKER_GENERATION));
    } else {
        let _ = reply.send(event.marker_generation);
    }
    registration
}

/// Runs `write` until its first marker is registered, then drops it.
async fn abandon_after_first_marker<F>(
    failures: &mut p2p::sync::PushFailureReceiver,
    pushed: &Atom<Vec<String>>,
    write: F,
) where
    F: std::future::Future,
{
    let first_marker = async {
        while let Some(event) = failures.recv().await {
            if answer(pushed, event) {
                return;
            }
        }
        panic!("the failure channel closed during the write");
    };
    if let Either::Left(_) = select(std::pin::pin!(write), std::pin::pin!(first_marker)).await {
        panic!("the write returned before registering a marker");
    }
}

type Mutator =
    BroadcastMutator<RegolithStore, DefraBlockstore<RegolithStore>, ConnectedPeerTransport>;

struct Node {
    db: Arc<DB<RegolithStore>>,
    mutator: Mutator,
    /// The DocIDs a marker was registered for.
    pushed: Atom<Vec<String>>,
    failures: p2p::sync::PushFailureReceiver,
}

/// A node whose only replicator takes articles and their chunks.
async fn node() -> Node {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    db.set_network(edge::testing::edge());
    db.create_collections_atomic(query::parse_sdl(SDL).unwrap())
        .await
        .unwrap();
    let id_of = |name: &str| {
        db.get_collection(name)
            .unwrap()
            .unwrap()
            .collection_id()
            .to_string()
    };
    let replicator = p2p::ReplicatorInfo::from_raw(
        PEER.to_string(),
        vec![id_of("Article"), id_of(CHUNKS)],
        vec![],
    );
    let transport =
        ConnectedPeerTransport::new("sender", PEER).listing_replicators(vec![replicator]);
    let (mut sync, _events) = SyncCoordinator::new(
        transport,
        Arc::new(DefraBlockstore::new(store, true)),
        p2p::network::Network::on(edge::testing::edge()),
        SyncConfig::default(),
    )
    .await
    .unwrap();
    let failures = sync.attach_failure_channel(FAILURE_CHANNEL_CAPACITY);
    Node {
        mutator: BroadcastMutator::new(db.clone(), Arc::new(sync)),
        db,
        pushed: Atom::new(Vec::new()),
        failures,
    }
}

impl Node {
    /// Answers registrations until `expected` are recorded or the grace ends.
    async fn registered_within_grace(&mut self, expected: usize) -> RapidHashSet<String> {
        let (failures, pushed) = (&mut self.failures, &self.pushed);
        let rest = async {
            while pushed.load().len() < expected {
                let Some(event) = failures.recv().await else {
                    return;
                };
                answer(pushed, event);
            }
        };
        let _ = workers::timeout(HANDOFF_GRACE, rest).await;
        self.pushed.swap(Vec::new()).iter().cloned().collect()
    }

    fn article(&self, body: &str) -> Document {
        let schema = self
            .db
            .get_collection("Article")
            .unwrap()
            .unwrap()
            .schema_arc();
        let mut doc = Document::new(schema);
        doc.set("body", NormalValue::String(body.into())).unwrap();
        doc
    }

    /// The DocIDs of `parent` and of every chunk it has.
    async fn with_chunks(&self, parent: &DocID) -> RapidHashSet<String> {
        let chunks = self.db.get_collection(CHUNKS).unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let docs = chunks
            .get_all_with_datastore_include_deleted(
                &txn.datastore().unwrap(),
                &txn.systemstore().unwrap(),
                false,
            )
            .await
            .unwrap();
        txn.force_discard().unwrap();
        docs.into_iter()
            .filter(|(chunk, _)| {
                chunk.get("_sourceID") == Some(&NormalValue::String(parent.to_string()))
            })
            .filter_map(|(chunk, _)| chunk.id().map(|id| id.to_string()))
            .chain(std::iter::once(parent.to_string()))
            .collect()
    }

    async fn only_article(&self) -> DocID {
        let articles = self.db.get_collection("Article").unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let docs = articles
            .get_all_with_datastore_include_deleted(
                &txn.datastore().unwrap(),
                &txn.systemstore().unwrap(),
                false,
            )
            .await
            .unwrap();
        txn.force_discard().unwrap();
        let [(doc, _)] = <[_; 1]>::try_from(docs).expect("exactly one article");
        doc.id().unwrap().clone()
    }
}

#[test]
fn a_create_registers_every_chunk_after_its_caller_stops_waiting() {
    workers::block_on(async {
        let mut node = node().await;
        let article = node.article(BODY);
        abandon_after_first_marker(
            &mut node.failures,
            &node.pushed,
            node.mutator.create("Article", article),
        )
        .await;

        let parent = node.only_article().await;
        let expected = node.with_chunks(&parent).await;
        assert!(
            expected.len() > 2,
            "the body must split into several chunks"
        );
        assert_eq!(node.registered_within_grace(expected.len()).await, expected);
    })
    .unwrap()
}

#[test]
fn a_batch_registers_every_chunk_after_its_caller_stops_waiting() {
    workers::block_on(async {
        let mut node = node().await;
        let article = node.article(BODY);
        let batch = node.mutator.begin_batch().await.unwrap().unwrap();
        let operation: query::mutator::MutationBatchOperation = Arc::new(move |mutator, _| {
            let article = article.clone();
            Box::pin(async move {
                mutator.create("Article", article).await?;
                Ok(serde_json::Value::Null)
            })
        });
        abandon_after_first_marker(&mut node.failures, &node.pushed, batch.run(operation)).await;

        let parent = node.only_article().await;
        let expected = node.with_chunks(&parent).await;
        assert!(
            expected.len() > 2,
            "the body must split into several chunks"
        );
        assert_eq!(node.registered_within_grace(expected.len()).await, expected);
    })
    .unwrap()
}
