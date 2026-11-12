//! Peer key requests against a peer KMS behind a counting transport with
//! injected latency: one wire request carries every missing key, concurrent
//! requests for a key share one, and a prefetched key is not asked again.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::{
    BlockDocIDResolver, DefraKms, EncodedFetchRequest, EncryptionCid, FetchEncryptionKeyRequest,
    IncomingHandler, KeyScope, KeyTransport, KmsService, MemoryKeyStore, PeerIdentity,
    PolicyDecision, RequestContext, TransportReplyStream,
};

const LATENCY: Duration = Duration::from_millis(100);

struct Owners(bool);

#[async_trait]
impl BlockDocIDResolver for Owners {
    async fn doc_ids_for_block(&self, _: &EncryptionCid) -> crate::Result<Vec<String>> {
        Ok(if self.0 {
            vec!["bae-owner".to_string()]
        } else {
            Vec::new()
        })
    }
}

struct AllowAll;

#[async_trait]
impl crate::AccessPolicy for AllowAll {
    async fn check_release(
        &self,
        _: Option<&identity::Did>,
        _: &KeyScope,
    ) -> crate::Result<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    async fn check_node_release(
        &self,
        _: Option<&identity::Did>,
        _: &KeyScope,
    ) -> crate::Result<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

fn node() -> identity::Did {
    "did:key:znode".parse().unwrap()
}

/// A transport whose far end is a peer KMS: counts wire requests and the
/// keys they ask for, and answers after `LATENCY`.
struct PeerTransport {
    peer: DefraKms,
    requests: AtomicUsize,
    links: AtomicUsize,
    /// The most keys one request asked for.
    largest: AtomicUsize,
    /// The keys per request this transport carries, if limited.
    cap: Option<usize>,
    /// Requests on the wire now, and the most at once.
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
}

#[async_trait]
impl KeyTransport for PeerTransport {
    fn name(&self) -> &'static str {
        "peer"
    }

    async fn send_request(
        &self,
        request: EncodedFetchRequest,
    ) -> crate::Result<TransportReplyStream> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let request: FetchEncryptionKeyRequest = defra_core::cbor::from_slice(&request.payload)
            .map_err(|error| crate::Error::WireDecode(error.to_string()))?;
        self.links.fetch_add(request.links.len(), Ordering::SeqCst);
        self.largest
            .fetch_max(request.links.len(), Ordering::SeqCst);
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_in_flight.fetch_max(now, Ordering::SeqCst);
        nuclei::time::sleep(LATENCY).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let from = PeerIdentity {
            peer_id: "peer".into(),
            authenticated_did: Some(node()),
            explicit_replay_authorization: None,
        };
        let reply = self.peer.serve_request(from, request).await;
        let (sender, stream) = crate::transport_reply_channel(1);
        let _ = sender
            .send(reply.map(|reply| (reply, "peer".to_string())))
            .await;
        Ok(stream)
    }

    fn install_handler(&self, _: Arc<dyn IncomingHandler>) {}

    fn max_keys_per_request(&self) -> Option<usize> {
        self.cap
    }
}

/// A peer holding `count` keys, and a node that has none and asks it.
async fn peer_and_node(
    count: usize,
    owned: bool,
) -> (Arc<PeerTransport>, DefraKms, Vec<EncryptionCid>) {
    peer_and_node_capped(count, owned, None).await
}

/// [`peer_and_node`], over a transport carrying at most `cap` keys per
/// request.
async fn peer_and_node_capped(
    count: usize,
    owned: bool,
    cap: Option<usize>,
) -> (Arc<PeerTransport>, DefraKms, Vec<EncryptionCid>) {
    let peer = DefraKms::new(
        Arc::new(MemoryKeyStore::new()),
        vec![],
        Arc::new(AllowAll),
        Arc::new(Owners(true)),
        node(),
    );
    peer.set_local_peer_id("peer".into());
    let mut cids = Vec::with_capacity(count);
    for index in 0..count {
        let (cid, _) = peer
            .generate_key(
                &RequestContext::anonymous(),
                KeyScope::Document {
                    doc_id: format!("doc-{index}"),
                    field: None,
                },
            )
            .await
            .unwrap();
        cids.push(cid);
    }
    let transport = Arc::new(PeerTransport {
        peer,
        requests: AtomicUsize::new(0),
        links: AtomicUsize::new(0),
        largest: AtomicUsize::new(0),
        cap,
        in_flight: AtomicUsize::new(0),
        most_in_flight: AtomicUsize::new(0),
    });
    let node_kms = DefraKms::new(
        Arc::new(MemoryKeyStore::new()),
        vec![transport.clone()],
        Arc::new(AllowAll),
        Arc::new(Owners(owned)),
        node(),
    );
    (transport, node_kms, cids)
}

#[nuclei::test(start_paused = true)]
async fn every_missing_key_of_a_call_goes_out_in_one_wire_request() {
    let (transport, kms, cids) = peer_and_node(5, true).await;
    let started = nuclei::time::Instant::now();
    let keys = kms
        .get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert!(keys.iter().all(Result::is_ok));
    assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
    assert_eq!(transport.links.load(Ordering::SeqCst), 5);
    assert_eq!(started.elapsed(), LATENCY, "one round trip, not five");

    // Now held locally: no further request.
    kms.get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
}

#[nuclei::test(start_paused = true)]
async fn concurrent_requests_for_one_key_make_one_wire_request() {
    let (transport, kms, cids) = peer_and_node(1, true).await;
    let kms = Arc::new(kms);
    let calls: Vec<_> = (0..16)
        .map(|_| {
            let kms = Arc::clone(&kms);
            let cids = cids.clone();
            nuclei::spawn(async move {
                kms.get_keys_each(&RequestContext::anonymous(), &cids)
                    .await
                    .unwrap()
            })
        })
        .collect();
    for call in calls {
        assert!(call.join().await.unwrap()[0].is_ok());
    }
    assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
}

#[nuclei::test(start_paused = true)]
async fn a_merge_after_a_prefetch_joins_it_instead_of_asking_again() {
    // Ownership unknown: a held key still defers to the owner, as a block
    // fetched during DAG sync does before its merge records ownership.
    let (transport, kms, cids) = peer_and_node(3, false).await;
    kms.prefetch_keys(&RequestContext::anonymous(), &cids).await;
    let keys = kms
        .get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert!(keys.iter().all(Result::is_ok));
    assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
}

/// A transport that carries at most two keys per request gets the five a
/// call asks for in three requests, sent together, and every key comes back.
#[nuclei::test(start_paused = true)]
async fn a_batch_is_split_to_the_keys_a_transport_carries() {
    let (transport, kms, cids) = peer_and_node_capped(5, true, Some(2)).await;
    let started = nuclei::time::Instant::now();
    let keys = kms
        .get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert!(keys.iter().all(Result::is_ok));
    assert_eq!(transport.requests.load(Ordering::SeqCst), 3);
    assert_eq!(transport.links.load(Ordering::SeqCst), 5);
    assert_eq!(transport.largest.load(Ordering::SeqCst), 2);
    assert_eq!(started.elapsed(), LATENCY, "the chunks go out together");
}

/// A transport with no limit of its own gets the whole batch in one
/// request, however many keys it holds.
#[nuclei::test(start_paused = true)]
async fn a_transport_without_a_limit_gets_the_batch_whole() {
    let (transport, kms, cids) = peer_and_node_capped(40, true, None).await;
    let keys = kms
        .get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert!(keys.iter().all(Result::is_ok));
    assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
    assert_eq!(transport.largest.load(Ordering::SeqCst), 40);
}

/// A batch split into more chunks than may be on the wire at once goes out
/// a few chunks at a time, and every key still comes back.
#[nuclei::test(start_paused = true)]
async fn a_split_batch_keeps_few_chunks_on_the_wire_at_once() {
    let (transport, kms, cids) = peer_and_node_capped(10, true, Some(1)).await;
    let started = nuclei::time::Instant::now();
    let keys = kms
        .get_keys_each(&RequestContext::anonymous(), &cids)
        .await
        .unwrap();
    assert!(keys.iter().all(Result::is_ok));
    assert_eq!(transport.requests.load(Ordering::SeqCst), 10);
    assert_eq!(
        transport.most_in_flight.load(Ordering::SeqCst),
        super::CHUNKS_IN_FLIGHT
    );
    assert_eq!(started.elapsed(), 3 * LATENCY, "ten chunks in waves of four");
}
