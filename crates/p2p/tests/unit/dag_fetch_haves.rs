use super::*;
use crate::sync::car::doc_dag::{updates, DocDag};
use crate::sync::coordinator::haves::haves_from_heads;

/// The receiver holds a document's history and is told of a new head: on the
/// steady path, a deep concurrent branch, and with nothing held. With its
/// haves named, each fetch is one request serving exactly what is missing; a
/// receiver holding nothing still gets the whole DAG.
#[nuclei::test(threads = 0)]
async fn a_fetch_naming_its_haves_is_served_only_the_missing_blocks() {
    let store = || {
        Arc::new(DefraBlockstore::new(
            Arc::new(RegolithStore::in_memory().unwrap()),
            true,
        ))
    };
    // (held updates, the provider's further updates, whether the receiver
    // made a concurrent update of its own)
    for (held, further, concurrent) in [(500, 1, false), (60, 200, true), (0, 40, false)] {
        let (provider, receiver) = (store(), store());
        let mut base = DocDag::default();
        let history = updates(&mut base, held, 1);
        let (mut theirs, mut ours) = (base.clone(), base.clone());
        let missing = updates(&mut theirs, further, 2);
        let local = if concurrent {
            updates(&mut ours, 1, 3)
        } else {
            Vec::new()
        };
        for (cid, data) in history.iter().chain(&missing) {
            provider.put(cid, data).await.unwrap();
        }
        for (cid, data) in history.iter().chain(&local) {
            receiver.put(cid, data).await.unwrap();
        }
        let haves = haves_from_heads(receiver.as_ref(), ours.heads()).await;
        let (head, head_data) = missing.last().unwrap().clone();
        let transport = TestTransport::new(
            receiver.clone(),
            head,
            head_data,
            RapidHashMap::new(),
            RapidHashMap::new(),
        );
        transport.set_force_rooted_sync();
        transport.set_closure_provider(provider);
        let (event_tx, mut event_rx) = mpsc::channel(1);

        poll_fetch_dag(
            transport.clone(),
            receiver.clone(),
            event_tx,
            head,
            DagFetchContext::new(
                "doc-id".to_string(),
                "collection-id".to_string(),
                "creator-id".to_string(),
                PeerId::new("remote-peer".to_string()),
            )
            .with_haves(haves),
            DagFetchLimiter::new(2),
            diagnostics(),
        )
        .await;

        assert!(matches!(
            event_rx.recv().await,
            Some(SyncEvent::DagReady { root_cid, .. }) if root_cid == head
        ));
        for (cid, _) in &missing {
            assert!(receiver.has(cid).await.unwrap());
        }
        if held > 0 {
            assert_eq!(
                transport.closure_rounds(),
                vec![(2, missing.len())],
                "held {held}"
            );
        } else {
            assert!(
                transport.closure_rounds().len() > 1,
                "nothing held keeps the depth cut"
            );
        }
    }
}
