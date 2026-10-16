use bitswap::server::ServerConfig;

mod common;
mod server_harness;
use common::block_v1;
use server_harness::{has_block, start, want_with};

#[tokio::test(start_paused = true)]
async fn ledgers_return_to_empty_after_churn_and_late_traffic() {
    let wanted = block_v1(b"wanted");
    let later = block_v1(b"later");
    let mut h = start(&[wanted.clone(), later.clone()], ServerConfig::default());
    assert_eq!(h.server.ledger_wants(h.peer).await, Some(0));

    h.send(want_with(wanted.cid, 1, false));
    h.server.peer_disconnected(h.peer);

    let messages = h.drain().await;
    assert!(has_block(&messages, &wanted.cid), "queued work still ships");
    assert_eq!(
        h.server.ledger_wants(h.peer).await,
        None,
        "the late sent report must not recreate the ledger"
    );

    h.send(want_with(later.cid, 1, false));
    let messages = h.drain().await;
    assert!(
        has_block(&messages, &later.cid),
        "a late message is still answered"
    );
    assert_eq!(h.server.ledger_wants(h.peer).await, None);
}
