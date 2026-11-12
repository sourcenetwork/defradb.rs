use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use storage::RegolithStore;

use super::car;
use super::tests::{create_composite_block, create_lww_block, TestTransport};
use crate::sync::{SyncConfig, SyncCoordinator};
use crate::transport::PeerId;

/// A response re-sending a block this node holds, while another ingest owns
/// that block, stores the new blocks instead of being dropped as contended,
/// and counts the held block.
#[nuclei::test(threads = 0)]
async fn a_held_block_in_a_response_is_skipped_not_contended() {
    let blockstore = Arc::new(DefraBlockstore::new(
        Arc::new(RegolithStore::in_memory().unwrap()),
        true,
    ));
    let (coordinator, _events) = SyncCoordinator::new(
        TestTransport::new(),
        blockstore.clone(),
        crate::network::test_network(),
        SyncConfig::default(),
    )
    .await
    .expect("coordinator");
    let (field_cid, field_block) = create_lww_block("name");
    let (root_cid, root_block) = create_composite_block("doc123", "name", field_cid);
    blockstore.put(&field_cid, &field_block).await.unwrap();
    let (other_root, _) = create_composite_block("doc456", "age", field_cid);
    let _other_owner = coordinator
        .manager()
        .try_acquire_car_storage_owners(other_root, vec![field_cid])
        .expect("the other ingest owns the held block");
    let car = crate::sync::car::encode_car(
        &[root_cid],
        &[(&root_cid, &root_block), (&field_cid, &field_block)],
    )
    .unwrap();

    let disposition = coordinator
        .handle_car_fetch_response(PeerId::new("provider".to_string()), root_cid, car)
        .await
        .unwrap();

    assert_eq!(disposition, car::CarIngestDisposition::Completed);
    assert!(blockstore.has(&root_cid).await.unwrap());
    let snapshot = coordinator.manager().diagnostics().snapshot();
    assert_eq!(snapshot.car_received_already_held, 1);
}
