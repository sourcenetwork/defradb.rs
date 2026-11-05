//! The height-only fetch before a create is walked (#2464).

use super::*;

/// A gate that judges a create by one linked block: with that block at hand
/// it refuses (or keeps) the document, without it the document goes on.
struct HeightAt {
    link: Cid,
    refuse: bool,
}

#[async_trait]
impl crate::sync::receive_gate::ReceiveGate for HeightAt {
    async fn judge(
        &self,
        _collection_id: &str,
        _doc_id: &str,
        _head: &[u8],
        blocks: &dyn crate::sync::receive_gate::GateBlocks,
    ) -> Option<crate::sync::receive_gate::GateRefusal> {
        let held = blocks.block(&self.link).await.is_some();
        (held && self.refuse).then_some(crate::sync::receive_gate::GateRefusal::BelowFloor)
    }

    async fn height_link(&self, _collection_id: &str, _head: &[u8]) -> Option<Cid> {
        Some(self.link)
    }
}

/// A create linking a height block and one other field block.
fn create_with_height(height: &[u8], name: &str) -> (Cid, Vec<u8>, Cid, Vec<u8>) {
    let height_cid = make_cid(height);
    let other = encode_ipld(ipld!({ "name": name }));
    let other_cid = make_cid(&other);
    let root = encode_ipld(ipld!({ "height": height_cid, "other": other_cid }));
    (make_cid(&root), root, other_cid, other)
}

fn height_gate(link: Cid, refuse: bool) -> crate::sync::coordinator::height_gate::HeightGate {
    crate::sync::coordinator::height_gate::HeightGate {
        gate: Arc::new(HeightAt { link, refuse }),
        fetches: crate::sync::queue::ProcessQueue::new(),
    }
}

async fn fetch_with_height_gate(
    transport: TestTransport,
    blockstore: Arc<DefraBlockstore<RegolithStore>>,
    root_cid: Cid,
    gate: crate::sync::coordinator::height_gate::HeightGate,
    diagnostics: Arc<SyncDiagnostics>,
) -> SyncEvent {
    let (event_tx, mut event_rx) = mpsc::channel(1);
    poll_fetch_dag(
        transport,
        blockstore,
        event_tx,
        root_cid,
        DagFetchContext::new(
            "doc-id".to_string(),
            "collection-id".to_string(),
            "creator-id".to_string(),
            PeerId::new("remote-peer".to_string()),
        )
        .with_height_gate(Some(gate)),
        DagFetchLimiter::new(2),
        diagnostics,
    )
    .await;
    event_rx.recv().await.expect("an outcome for the root")
}

/// With the CAR exchange failing, a create the gate refuses on its height
/// block is never walked: only its head and its height block are fetched,
/// and the root is handed back as `retentionAfterHeightFetch`.
#[nuclei::test(threads = 0)]
async fn a_create_refused_on_its_fetched_height_block_is_not_walked() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let height = encode_ipld(ipld!({ "height": 5 }));
    let height_cid = make_cid(&height);
    let (root_cid, root_data, other_cid, other) = create_with_height(&height, "a");
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(height_cid, height), (other_cid, other)]),
    );
    let diagnostics = diagnostics();

    let event = fetch_with_height_gate(
        transport.clone(),
        blockstore.clone(),
        root_cid,
        height_gate(height_cid, true),
        diagnostics.clone(),
    )
    .await;

    assert!(matches!(
        event,
        SyncEvent::GateRefused { root_cid: refused, skip, .. }
            if refused == root_cid
                && skip == crate::sync::manager::arrival_stats::SkipReason::RetentionAfterHeightFetch
    ));
    assert!(matches!(blockstore.has(&height_cid).await, Ok(true)));
    assert!(matches!(blockstore.has(&other_cid).await, Ok(false)));
    assert!(!transport
        .sync_batches()
        .iter()
        .any(|batch| batch.contains(&other_cid)));
    let report = diagnostics.arrivals().drain();
    assert_eq!((report.height_fetches, report.height_fetch_missed), (1, 0));
}

/// A create the gate keeps on its height block is walked as before.
#[nuclei::test(threads = 0)]
async fn a_create_kept_on_its_fetched_height_block_is_walked() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let height = encode_ipld(ipld!({ "height": 500 }));
    let height_cid = make_cid(&height);
    let (root_cid, root_data, other_cid, other) = create_with_height(&height, "a");
    let transport = TestTransport::new(
        blockstore.clone(),
        root_cid,
        root_data,
        RapidHashMap::new(),
        RapidHashMap::from_iter([(height_cid, height), (other_cid, other)]),
    );
    let diagnostics = diagnostics();

    let event = fetch_with_height_gate(
        transport,
        blockstore.clone(),
        root_cid,
        height_gate(height_cid, false),
        diagnostics.clone(),
    )
    .await;

    assert!(matches!(event, SyncEvent::DagReady { root_cid: ready, .. } if ready == root_cid));
    assert!(matches!(blockstore.has(&other_cid).await, Ok(true)));
    assert_eq!(diagnostics.arrivals().drain().height_fetches, 1);
}

/// Two creates at one height share their height block: it is asked of the
/// network once, whether the second fetch waits on the first or finds it
/// stored.
#[nuclei::test(threads = 0)]
async fn a_height_block_two_creates_share_is_fetched_once() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let blockstore = Arc::new(DefraBlockstore::new(store, true));
    let height = encode_ipld(ipld!({ "height": 5 }));
    let height_cid = make_cid(&height);
    let (root_a, data_a, other_a, block_a) = create_with_height(&height, "a");
    let (root_b, data_b, other_b, block_b) = create_with_height(&height, "b");
    let served =
        RapidHashMap::from_iter([(height_cid, height), (other_a, block_a), (other_b, block_b)]);
    let transport_a = TestTransport::new(
        blockstore.clone(),
        root_a,
        data_a,
        RapidHashMap::new(),
        served.clone(),
    );
    let transport_b = TestTransport::new(
        blockstore.clone(),
        root_b,
        data_b,
        RapidHashMap::new(),
        served,
    );
    let gate = height_gate(height_cid, true);
    let diagnostics = diagnostics();

    let (a, b) = futures::join!(
        fetch_with_height_gate(
            transport_a.clone(),
            blockstore.clone(),
            root_a,
            gate.clone(),
            diagnostics.clone(),
        ),
        fetch_with_height_gate(
            transport_b.clone(),
            blockstore.clone(),
            root_b,
            gate,
            diagnostics.clone(),
        ),
    );

    assert!(matches!(a, SyncEvent::GateRefused { .. }));
    assert!(matches!(b, SyncEvent::GateRefused { .. }));
    let asked = [transport_a, transport_b]
        .iter()
        .flat_map(|transport| transport.sync_batches())
        .filter(|batch| batch.contains(&height_cid))
        .count();
    assert_eq!(asked, 1);
    assert_eq!(diagnostics.arrivals().drain().height_fetches, 1);
}
