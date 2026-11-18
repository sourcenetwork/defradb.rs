use std::sync::Arc;

use p2p::BitswapStoreAdapter;

use super::tests::{identity_test_adapter, NoopBlockstore};
use crate::{P2POperations, P2pDocumentRequest};

#[nuclei::test]
async fn remove_documents_clears_the_shared_followed_set() {
    let (host, handle, _events, _replicators) = p2p::host::P2PHost::new(
        BitswapStoreAdapter::new(Arc::new(NoopBlockstore)),
        edge::testing::edge(),
    )
    .await
    .unwrap();
    let task = p2p::rt::spawn(host.run());
    let mut adapter = identity_test_adapter(handle);
    let followed = Arc::new(p2p::sync::FollowedDocuments::with_hasher(
        rapidhash::fast::RandomState::default(),
    ));
    adapter.tracked_documents = Arc::clone(&followed);
    let doc_id = document::DocID::new_v0_from_seed("followed").to_string();
    let request = || {
        vec![P2pDocumentRequest {
            collection: String::new(),
            doc_id: doc_id.clone(),
        }]
    };

    adapter.add_documents(request()).await.unwrap();
    assert!(followed.contains_key(&doc_id));

    adapter.remove_documents(request()).await.unwrap();
    assert!(!followed.contains_key(&doc_id));
    adapter.handle.shutdown().await.unwrap();
    task.join().await.unwrap();
}
