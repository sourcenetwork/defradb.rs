//! Searchable-encryption serve side (replicator) and artifact receive helper.
//!
//! A replicator stores SE artifacts pushed by document owners and answers their
//! `QuerySEArtifactsRequest`s by byte-matching the request's search tags against
//! its locally stored artifacts (`fetch_doc_ids`). This is the consumer half of
//! Go's `internal/se` producer-consumer model.
//!
//! Both the standalone CLI and the node route inbound SE events here so
//! the serve/receive logic lives in one place.

use std::future::Future;
use std::sync::Arc;

use defra_core::thread_bounds::MaybeSend;
#[cfg(feature = "p2p")]
use p2p::message::PushSEArtifactsReply;
use p2p::message::{QuerySEArtifactsReply, QuerySEArtifactsRequest};
use p2p::transport::{P2PTransport, PeerId};
#[cfg(feature = "p2p")]
use p2p::P2PHostHandle;
use storage::corekv::Store;

use super::receive_and_store;

/// Runs `work` on `pool` when the node has one, so a peer's SE request reads
/// and writes storage on a worker, never on the network thread that took it.
async fn on_pool<S, T, F, Fut>(
    pool: Option<&workers::Pool>,
    store: &Arc<S>,
    work: F,
) -> std::result::Result<T, String>
where
    S: Store + 'static,
    F: FnOnce(Arc<S>) -> Fut + MaybeSend + 'static,
    Fut: Future<Output = std::result::Result<T, String>> + 'static,
    T: MaybeSend + 'static,
{
    let store = Arc::clone(store);
    #[cfg(not(target_arch = "wasm32"))]
    {
        workers::run_on(pool, workers::Target::LeastLoaded, move || work(store))
            .await
            .map_err(|error| error.report("SE storage call").to_string())?
    }
    // A browser's one worker is the page's thread: the call runs where it is
    // made.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = pool;
        work(store).await
    }
}
use super::storage::{fetch_doc_ids, FieldQuery};

/// Handle an inbound `QuerySEArtifactsRequest` on the serving (replicator) node.
///
/// Opens a read transaction, byte-matches the request's search tags against
/// locally stored artifacts, and sends back a **signed** reply. On failure a
/// signed error reply is sent so the requester's correlator slot resolves
/// instead of timing out. The reply goes back on the request's stream when
/// `token` carries one.
pub async fn handle_query_request<S, T>(
    store: &Arc<S>,
    pool: Option<&workers::Pool>,
    transport: &T,
    peer_id: PeerId,
    request: QuerySEArtifactsRequest,
    token: Option<T::ResponseToken>,
) where
    S: Store + 'static,
    T: P2PTransport,
{
    let query = request.clone();
    let built = on_pool(pool, store, move |store| async move {
        build_reply(&*store, &query).await
    })
    .await;
    let reply = match built {
        Ok(doc_ids) => QuerySEArtifactsReply::success(&request.message_id, doc_ids),
        Err(error) => {
            tracing::warn!(
                peer_id = %peer_id,
                message_id = %request.message_id,
                collection_id = %request.collection_id,
                error = %error,
                "SE query serve failed; replying with error"
            );
            QuerySEArtifactsReply::error(&request.message_id, &error)
        }
    };

    let mut reply = reply;
    if let Err(error) = p2p::signing::sign_with_transport(transport, &mut reply) {
        tracing::warn!(peer_id = %peer_id, error = %error, "failed to sign SE query reply");
        return;
    }

    if let Err(error) = transport
        .send_se_query_response(&peer_id, token, reply)
        .await
    {
        tracing::warn!(peer_id = %peer_id, error = %error, "failed to send SE query reply");
    }
}

async fn build_reply<S: Store>(
    store: &S,
    request: &QuerySEArtifactsRequest,
) -> std::result::Result<Vec<String>, String> {
    // Artifacts live in the datastore namespace, where truncate and purge
    // delete them (Go's `DatastoreSE` keys go through the datastore too).
    let txn = datastore::BasicTxn::new(store, 0, true)
        .await
        .map_err(|e| format!("failed to open SE read txn: {e}"))?;

    let queries: Vec<FieldQuery> = request
        .queries
        .iter()
        .map(|q| FieldQuery::new(&q.field_name, &q.index_id, q.search_tag.clone()))
        .collect();

    let doc_ids = fetch_doc_ids(&txn.datastore(), &request.collection_id, &queries)
        .await
        .map_err(|e| format!("SE artifact lookup failed: {e}"));
    // A read-only transaction has nothing to keep; discarding it cannot lose
    // a write, so a failure is only traced.
    if let Err(error) = txn.discard() {
        tracing::warn!(%error, "failed to discard a read-only transaction");
    }
    let doc_ids = doc_ids?;

    tracing::debug!(
        collection_id = %request.collection_id,
        query_count = queries.len(),
        match_count = doc_ids.len(),
        "served SE query"
    );

    Ok(doc_ids)
}

/// Receive, validate, and store SE artifacts pushed by a document owner.
///
/// Shared by the CLI and embedded event loops (both forward
/// `SEArtifactsReceived` here). Returns the stored doc IDs so callers can emit
/// `se_artifact_received` bus events.
pub async fn handle_artifacts_received<S>(
    store: &Arc<S>,
    pool: Option<&workers::Pool>,
    peer_id: &str,
    data: &[u8],
) -> Vec<String>
where
    S: Store + 'static,
{
    let (peer, data) = (peer_id.to_owned(), data.to_vec());
    let stored = on_pool(pool, store, move |store| async move {
        Ok(store_artifacts(&*store, &peer, &data).await)
    })
    .await;
    stored.unwrap_or_else(|error| {
        tracing::warn!(peer_id = %peer_id, error = %error, "failed to receive SE artifacts");
        Vec::new()
    })
}

/// Receives, validates and stores `data` in one transaction: the stored doc
/// IDs, or none when it failed, which is logged.
async fn store_artifacts<S>(store: &S, peer_id: &str, data: &[u8]) -> Vec<String>
where
    S: Store,
{
    // The datastore namespace, as `build_reply` reads and truncate and purge
    // delete.
    let txn = match datastore::BasicTxn::new(store, 0, false).await {
        Ok(txn) => txn,
        Err(error) => {
            tracing::warn!(peer_id = %peer_id, error = %error, "failed to create SE artifact transaction");
            return Vec::new();
        }
    };

    let result = match receive_and_store(&mut txn.datastore(), data).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(peer_id = %peer_id, error = %error, "failed to receive SE artifacts");
            return Vec::new();
        }
    };

    if let Err(error) = txn.commit().await {
        tracing::warn!(peer_id = %peer_id, error = %error, "failed to commit SE artifacts");
        return Vec::new();
    }

    tracing::debug!(
        peer_id = %peer_id,
        collection_id = %result.collection_id,
        stored = result.stored,
        rejected = result.rejected,
        "stored incoming SE artifacts"
    );

    result.doc_ids
}

/// Extract the `MessageID` from a CBOR-encoded `PushSEArtifactsRequest`.
#[cfg(feature = "p2p")]
fn extract_push_message_id(data: &[u8]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct MsgIdOnly {
        #[serde(rename = "MessageID")]
        message_id: String,
    }
    defra_core::cbor::from_slice::<MsgIdOnly>(data)
        .ok()
        .map(|m| m.message_id)
}

/// Store inbound SE artifacts AND send a signed `PushSEArtifactsReply` ack.
///
/// Go's artifact push (`storeSEProto.SendRequest`) blocks until it receives this
/// reply, so a Rust replicator MUST acknowledge or the Go owner's write hangs.
/// Returns the stored doc IDs for bus events.
#[cfg(feature = "p2p")]
pub async fn handle_artifacts_push<S>(
    store: &Arc<S>,
    pool: Option<&workers::Pool>,
    handle: &P2PHostHandle,
    peer_id: p2p::PeerId,
    data: &[u8],
) -> Vec<String>
where
    S: Store + 'static,
{
    let doc_ids = handle_artifacts_received(store, pool, &peer_id.to_string(), data).await;

    if let Some(message_id) = extract_push_message_id(data) {
        let mut reply = PushSEArtifactsReply::success(&message_id);
        match p2p::sign_message(handle.keypair(), &mut reply) {
            Ok(()) => {
                if let Err(error) = handle.send_se_artifacts_response(peer_id, reply).await {
                    tracing::warn!(peer_id = %peer_id, error = %error, "failed to ack SE artifacts push");
                }
            }
            Err(error) => {
                tracing::warn!(peer_id = %peer_id, error = %error, "failed to sign SE artifacts ack");
            }
        }
    } else {
        tracing::warn!(peer_id = %peer_id, "SE artifacts push missing MessageID; cannot ack");
    }

    doc_ids
}

#[cfg(all(test, feature = "p2p", not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use defra_core::RequestContext;
    use document::{Document, NormalValue};
    use p2p::message::{PushSEArtifactsRequest, SEArtifact, SEFieldQuery};
    use query::DocMutator;
    use schema::CollectionVersion;
    use storage::corekv::{IterOptions, Iterator, Reader, Store};
    use storage::RegolithStore;

    use super::*;
    use crate::{AutoCommitMutator, DB};

    const TAG: [u8; 16] = [7; 16];

    const USERS_SDL: &str = "type Users { name: String age: Int }";

    fn users_schema() -> CollectionVersion {
        query::parse_sdl(USERS_SDL).unwrap().remove(0)
    }

    fn collection_id() -> String {
        users_schema().collection_id
    }

    /// Pushes one artifact per doc the way a document owner does, through
    /// the libp2p push entry point the node's event loop calls.
    async fn push(store: &Arc<RegolithStore>, handle: &P2PHostHandle, doc_ids: &[&str]) {
        let artifacts = doc_ids
            .iter()
            .map(|doc_id| SEArtifact::new(*doc_id, "name", TAG.to_vec()))
            .collect();
        let mut request = PushSEArtifactsRequest::new(collection_id(), artifacts);
        request.message_id = "push-1".into();
        let data = defra_core::cbor::to_vec(&request).unwrap();
        let mut stored =
            handle_artifacts_push(store, None, handle, p2p::PeerId::random(), &data).await;
        stored.sort();
        assert_eq!(stored, doc_ids);
    }

    async fn served(store: &Arc<RegolithStore>) -> Vec<String> {
        let request = QuerySEArtifactsRequest::new(
            collection_id(),
            vec![SEFieldQuery::new("name", "name", TAG.to_vec())],
        );
        let mut doc_ids = build_reply(&**store, &request).await.unwrap();
        doc_ids.sort();
        doc_ids
    }

    /// Every `/se/` key, in the root keyspace and in the datastore namespace.
    async fn se_keys(store: &RegolithStore) -> Vec<Vec<u8>> {
        let txn = store.new_txn(true).await.unwrap();
        let mut keys = Vec::new();
        for prefix in [b"/se/".to_vec(), b"d/se/".to_vec()] {
            let mut iter = txn
                .iterator(IterOptions::new().with_prefix(prefix))
                .await
                .unwrap();
            while let Some(pair) = iter.next().await.unwrap() {
                keys.push(pair.key.to_vec());
            }
            iter.close().await.unwrap();
        }
        keys
    }

    #[test]
    fn received_se_artifacts_removed_by_truncate() {
        workers::block_on(edge::compat(async {
            let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
            db.add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
                .await
                .unwrap();
            let store = Arc::clone(db.store());
            // A host that is not running: the push's ack fails and is
            // logged, which leaves the stored artifacts as they are.
            let (_, handle, _events, _replicators) = p2p::P2PHost::new(
                p2p::testutil::MockBitswapStore::new(),
                edge::testing::edge(),
            )
            .await
            .unwrap();

            let mut doc = Document::new(users_schema());
            doc.set("name", NormalValue::String("alice".into()))
                .unwrap();
            doc.set("age", NormalValue::Int(30)).unwrap();
            let alice = AutoCommitMutator::new(db.clone())
                .create("Users", doc)
                .await
                .unwrap()
                .doc_id
                .to_string();

            push(&store, &handle, &[&alice]).await;
            assert_eq!(served(&store).await, vec![alice.clone()]);

            let filter: query::documents::Filter = serde_json::json!({"age": {"_eq": 30}}).into();
            db.truncate_collection_as(&RequestContext::ambient(), "Users", Some(&filter))
                .await
                .unwrap();
            assert!(served(&store).await.is_empty());
            assert!(se_keys(&store).await.is_empty());

            // A replicator also holds artifacts for documents it never
            // received; only the collection-wide truncate reaches those.
            let unheld = "bae-00000000-0000-0000-0000-000000000000";
            push(&store, &handle, &[unheld]).await;
            assert_eq!(served(&store).await, vec![unheld.to_owned()]);

            db.truncate_collection_as(&RequestContext::ambient(), "Users", None)
                .await
                .unwrap();
            assert!(served(&store).await.is_empty());
            assert!(se_keys(&store).await.is_empty());
        }))
        .unwrap()
    }
}
