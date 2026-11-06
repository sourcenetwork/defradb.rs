//! The single-write `BroadcastMutator` pushes a filtered replicator the chunks
//! of the parents its filter matches, on create, update and delete.

use std::sync::Arc;

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

const SDL: &str = "type Article { kind: String body: String @chunk(maxChars: 64) }";
const CHUNKS: &str = "Article__body_chunk";
const PEER: &str = "receiver";
const PUBLIC: &str = "public";
const SECRET: &str = "secret";
const FAILURE_CHANNEL_CAPACITY: usize = 64;

type Mutator =
    BroadcastMutator<RegolithStore, DefraBlockstore<RegolithStore>, ConnectedPeerTransport>;

struct Node {
    db: Arc<DB<RegolithStore>>,
    mutator: Mutator,
    /// The DocIDs a push job was registered for, so the peer is owed them.
    owed: Atom<Vec<String>>,
    failures: p2p::sync::PushFailureReceiver,
}

// A node whose only replicator takes public articles, with their chunks.
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
    let (articles, chunks) = (id_of("Article"), id_of(CHUNKS));
    let replicator = p2p::ReplicatorInfo::from_raw_with_filters(
        PEER.to_string(),
        vec![articles.clone(), chunks.clone()],
        vec![],
        [(
            articles.clone(),
            p2p::ReplicationFilter::eq("kind", serde_json::json!(PUBLIC)),
        )]
        .into(),
    )
    .with_chunk_sources([(chunks, articles)]);
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
        owed: Atom::new(Vec::new()),
        failures,
    }
}

/// Runs `write` while answering the coordinator's marker registrations, and
/// records each DocID it registers a push for.
async fn recording<F, T>(
    failures: &mut p2p::sync::PushFailureReceiver,
    owed: &Atom<Vec<String>>,
    write: F,
) -> T
where
    F: std::future::Future<Output = T>,
{
    let recorder = async {
        while let Some(mut event) = failures.recv().await {
            let Some(reply) = event.durable_tx.take() else {
                continue;
            };
            let registration = !event.admission_only && !event.acknowledged && !event.create_retry;
            if registration {
                owed.rcu(|docs| {
                    let mut next = docs.clone();
                    next.push(event.doc_id.clone());
                    next
                });
                let _ = reply.send(Some(1));
            } else {
                let _ = reply.send(event.marker_generation);
            }
        }
    };
    match select(std::pin::pin!(write), std::pin::pin!(recorder)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => panic!("the failure channel closed during the write"),
    }
}

impl Node {
    fn take_owed(&self) -> RapidHashSet<String> {
        self.owed.swap(Vec::new()).iter().cloned().collect()
    }

    fn article(&self, kind: &str, body: &str) -> Document {
        let schema = self
            .db
            .get_collection("Article")
            .unwrap()
            .unwrap()
            .schema_arc();
        let mut doc = Document::new(schema);
        doc.set("kind", NormalValue::String(kind.into())).unwrap();
        doc.set("body", NormalValue::String(body.into())).unwrap();
        doc
    }
}

impl Node {
    /// The DocIDs of every chunk `parent` ever had, deleted ones too.
    async fn chunks_of(&self, parent: &DocID) -> RapidHashSet<String> {
        let chunks = self.db.get_collection(CHUNKS).unwrap().unwrap();
        let txn = self.db.new_txn(true).await.unwrap();
        let docs = chunks
            .get_all_with_datastore_include_deleted(
                &txn.datastore().unwrap(),
                &txn.systemstore().unwrap(),
                true,
            )
            .await
            .unwrap();
        txn.force_discard().unwrap();
        docs.into_iter()
            .filter(|(chunk, _)| {
                chunk.get("_sourceID") == Some(&NormalValue::String(parent.to_string()))
            })
            .filter_map(|(chunk, _)| chunk.id().map(|id| id.to_string()))
            .collect()
    }
}

fn with_parent(parent: &DocID, chunks: RapidHashSet<String>) -> RapidHashSet<String> {
    let mut docs = chunks;
    docs.insert(parent.to_string());
    docs
}

#[test]
fn a_create_pushes_only_the_chunks_of_a_matching_parent() {
    workers::block_on(async {
        let mut node = node().await;
        let (public, secret) = (
            node.article(PUBLIC, "Open text."),
            node.article(SECRET, "Hidden text."),
        );

        let public = recording(
            &mut node.failures,
            &node.owed,
            node.mutator.create("Article", public),
        )
        .await
        .unwrap()
        .doc_id;
        recording(
            &mut node.failures,
            &node.owed,
            node.mutator.create("Article", secret),
        )
        .await
        .unwrap();

        let expected = with_parent(&public, node.chunks_of(&public).await);
        assert_eq!(node.take_owed(), expected);
    })
    .unwrap()
}

#[test]
fn an_update_pushes_the_new_chunks_of_a_matching_parent() {
    workers::block_on(async {
        let mut node = node().await;
        let created = node.article(PUBLIC, "Open text.");
        let public = recording(
            &mut node.failures,
            &node.owed,
            node.mutator.create("Article", created),
        )
        .await
        .unwrap()
        .doc_id;
        let before = node.chunks_of(&public).await;
        node.take_owed();
        let mut doc = node
            .mutator
            .get_for_update("Article", &public)
            .await
            .unwrap()
            .unwrap();
        doc.set("body", NormalValue::String("Edited text.".into()))
            .unwrap();

        let body = RapidHashSet::from_iter(["body".to_string()]);
        recording(
            &mut node.failures,
            &node.owed,
            node.mutator.update("Article", doc, body),
        )
        .await
        .unwrap();

        let added = node
            .chunks_of(&public)
            .await
            .difference(&before)
            .cloned()
            .collect();
        assert_eq!(node.take_owed(), with_parent(&public, added));
    })
    .unwrap()
}

#[test]
fn a_delete_pushes_the_chunk_deletes_of_a_matching_parent_only() {
    workers::block_on(async {
        let mut node = node().await;
        let (public, secret) = (
            node.article(PUBLIC, "Open text."),
            node.article(SECRET, "Hidden text."),
        );
        let public = recording(
            &mut node.failures,
            &node.owed,
            node.mutator.create("Article", public),
        )
        .await
        .unwrap()
        .doc_id;
        let secret = recording(
            &mut node.failures,
            &node.owed,
            node.mutator.create("Article", secret),
        )
        .await
        .unwrap()
        .doc_id;
        node.take_owed();

        recording(
            &mut node.failures,
            &node.owed,
            node.mutator.delete("Article", &public),
        )
        .await
        .unwrap();
        recording(
            &mut node.failures,
            &node.owed,
            node.mutator.delete("Article", &secret),
        )
        .await
        .unwrap();

        let expected = with_parent(&public, node.chunks_of(&public).await);
        assert_eq!(node.take_owed(), expected);
    })
    .unwrap()
}
