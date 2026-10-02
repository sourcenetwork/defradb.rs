use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::time::Duration;

use bitswap::network::{Network, OutEvent, SendError};
use bitswap::server::{Server, ServerConfig};
use bitswap::{BitswapMessage, Block, WantType};
use cid::Cid;
use libp2p::PeerId;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

mod common;
use common::mem_store::MemStore;
use common::{block_v1, cid_v1};

struct Sent {
    message: BitswapMessage,
    response: oneshot::Sender<Result<(), SendError>>,
}

struct Harness {
    server: Server,
    sent: mpsc::UnboundedReceiver<Sent>,
    peer: PeerId,
}

fn start(blocks: &[Block], config: ServerConfig) -> Harness {
    let (network, mut events) = Network::new(PeerId::random());
    let (tx, sent) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match poll_fn(|cx| events.poll_next(cx)).await {
                OutEvent::Dial { response, .. } => {
                    let _ = response.send(Ok(None));
                }
                OutEvent::SendMessage {
                    message, response, ..
                } => {
                    let _ = tx.send(Sent { message, response });
                }
                OutEvent::Protect { .. } | OutEvent::Unprotect { .. } => {}
            }
        }
    });
    Harness {
        server: Server::new(network, MemStore::new(blocks), config),
        sent,
        peer: PeerId::random(),
    }
}

impl Harness {
    fn send(&mut self, message: BitswapMessage) {
        self.server
            .try_receive_message(self.peer, message)
            .expect("inbound queue has room");
    }

    async fn next(&mut self) -> Sent {
        timeout(Duration::from_secs(5), self.sent.recv())
            .await
            .expect("a message within the deadline")
            .expect("the capture task is alive")
    }

    /// Collects messages, acknowledging each, until one carries the sentinel block.
    async fn until_sentinel(&mut self, sentinel: &Cid) -> Vec<BitswapMessage> {
        let mut out = Vec::new();
        loop {
            let sent = self.next().await;
            let _ = sent.response.send(Ok(()));
            let done = sent.message.blocks().any(|b| b.cid == *sentinel);
            out.push(sent.message);
            if done {
                return out;
            }
        }
    }
}

fn want(cid: Cid, want_type: WantType, send_dont_have: bool) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_entry(cid, 1, want_type, send_dont_have);
    message
}

fn cancel(cid: Cid) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.cancel(cid);
    message
}

fn has_block(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.blocks().any(|b| b.cid == *cid))
}

fn has_dont_have(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.dont_haves().any(|c| c == cid))
}

fn has_have(messages: &[BitswapMessage], cid: &Cid) -> bool {
    messages.iter().any(|m| m.haves().any(|c| c == cid))
}

#[tokio::test(start_paused = true)]
async fn want_have_of_small_block_is_answered_with_the_block() {
    let small = block_v1(&[1; 100]);
    let mut h = start(std::slice::from_ref(&small), ServerConfig::default());

    h.send(want(small.cid, WantType::Have, false));
    let messages = h.until_sentinel(&small.cid).await;

    assert!(!has_have(&messages, &small.cid));
    assert_eq!(messages[0].pending_bytes(), 0);
}

#[tokio::test(start_paused = true)]
async fn want_have_of_block_over_replace_size_is_answered_with_have() {
    let big = block_v1(&[2; 1025]);
    let sentinel = block_v1(b"sentinel");
    let mut h = start(&[big.clone(), sentinel.clone()], ServerConfig::default());

    h.send(want(big.cid, WantType::Have, false));
    let first = h.next().await;
    let _ = first.response.send(Ok(()));
    assert!(first.message.haves().any(|c| *c == big.cid));
    assert_eq!(first.message.blocks_len(), 0);

    h.send(want(big.cid, WantType::Block, false));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;
    assert!(has_block(&messages, &big.cid));
}

#[tokio::test(start_paused = true)]
async fn missing_want_block_with_send_dont_have_is_answered_with_dont_have() {
    let missing = cid_v1(b"missing");
    let mut h = start(&[], ServerConfig::default());

    h.send(want(missing, WantType::Block, true));
    let sent = h.next().await;
    let _ = sent.response.send(Ok(()));

    assert!(sent.message.dont_haves().any(|c| *c == missing));
    assert_eq!(sent.message.blocks_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn missing_want_without_send_dont_have_is_not_answered() {
    let missing = cid_v1(b"missing");
    let sentinel = block_v1(b"sentinel");
    let mut h = start(std::slice::from_ref(&sentinel), ServerConfig::default());

    h.send(want(missing, WantType::Block, false));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;

    assert!(!has_dont_have(&messages, &missing));
}

#[tokio::test(start_paused = true)]
async fn server_without_dont_haves_never_sends_them() {
    let missing = cid_v1(b"missing");
    let sentinel = block_v1(b"sentinel");
    let config = ServerConfig {
        send_dont_haves: false,
        ..Default::default()
    };
    let mut h = start(std::slice::from_ref(&sentinel), config);

    h.send(want(missing, WantType::Block, true));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;

    assert!(!has_dont_have(&messages, &missing));
}

fn deny(denied: Cid) -> Box<dyn bitswap::PeerBlockRequestFilter> {
    Box::new(
        move |_: &PeerId, cid: &Cid| -> Pin<Box<dyn Future<Output = bool> + Send>> {
            let allowed = *cid != denied;
            Box::pin(async move { allowed })
        },
    )
}

#[tokio::test(start_paused = true)]
async fn filter_denied_want_gets_dont_have_even_when_the_block_exists() {
    let secret = block_v1(b"secret");
    let mut h = start(
        std::slice::from_ref(&secret),
        ServerConfig {
            peer_block_request_filter: Some(deny(secret.cid)),
            ..Default::default()
        },
    );

    h.send(want(secret.cid, WantType::Block, true));
    let sent = h.next().await;
    let _ = sent.response.send(Ok(()));

    assert!(sent.message.dont_haves().any(|c| *c == secret.cid));
    assert_eq!(sent.message.blocks_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn filter_denied_want_without_send_dont_have_is_silent() {
    let secret = block_v1(b"secret");
    let sentinel = block_v1(b"sentinel");
    let mut h = start(
        &[secret.clone(), sentinel.clone()],
        ServerConfig {
            peer_block_request_filter: Some(deny(secret.cid)),
            ..Default::default()
        },
    );

    h.send(want(secret.cid, WantType::Block, false));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;

    assert!(!has_block(&messages, &secret.cid));
    assert!(!has_dont_have(&messages, &secret.cid));
}

#[tokio::test(start_paused = true)]
async fn cancel_before_the_tick_removes_the_task() {
    let wanted = block_v1(b"wanted");
    let sentinel = block_v1(b"sentinel");
    let mut h = start(&[wanted.clone(), sentinel.clone()], ServerConfig::default());

    h.send(want(wanted.cid, WantType::Block, false));
    h.send(cancel(wanted.cid));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;

    assert!(!has_block(&messages, &wanted.cid));
}

#[tokio::test(start_paused = true)]
async fn full_wantlist_clears_the_ledger_so_a_later_cancel_finds_nothing() {
    let wanted = block_v1(b"wanted");
    let sentinel = block_v1(b"sentinel");
    let mut h = start(&[wanted.clone(), sentinel.clone()], ServerConfig::default());

    h.send(want(wanted.cid, WantType::Block, false));
    h.send(BitswapMessage::new(true));
    h.send(cancel(wanted.cid));
    h.send(want(sentinel.cid, WantType::Block, false));
    let messages = h.until_sentinel(&sentinel.cid).await;

    assert!(has_block(&messages, &wanted.cid));
}

#[tokio::test(start_paused = true)]
async fn outstanding_cap_holds_back_work_until_tasks_are_done() {
    const SIZE: usize = 600_000;
    let blocks: Vec<Block> = (1..=3u8).map(|i| block_v1(&vec![i; SIZE])).collect();
    let mut h = start(&blocks, ServerConfig::default());

    let mut message = BitswapMessage::new(false);
    for block in &blocks {
        message.add_entry(block.cid, 1, WantType::Block, false);
    }
    h.send(message);

    let first = h.next().await;
    let second = h.next().await;
    let mut pending = [
        first.message.pending_bytes(),
        second.message.pending_bytes(),
    ];
    pending.sort_unstable();
    assert_eq!(pending, [SIZE as i32, 2 * SIZE as i32]);
    assert_eq!(first.message.blocks_len(), 1);
    assert_eq!(second.message.blocks_len(), 1);

    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(h.sent.try_recv().is_err(), "cap must hold the third block");

    let Sent {
        message: first_message,
        response,
    } = first;
    let _ = response.send(Ok(()));
    let third = h.next().await;
    assert_eq!(third.message.pending_bytes(), 0);
    assert_eq!(third.message.blocks_len(), 1);

    let mut cids: Vec<Cid> = [&first_message, &second.message, &third.message]
        .iter()
        .flat_map(|m| m.blocks().map(|b| b.cid))
        .collect();
    cids.sort();
    let mut expected: Vec<Cid> = blocks.iter().map(|b| b.cid).collect();
    expected.sort();
    assert_eq!(cids, expected);
}

#[tokio::test(start_paused = true)]
async fn disconnect_drops_the_ledger_but_not_queued_work() {
    let wanted = block_v1(b"wanted");
    let mut h = start(std::slice::from_ref(&wanted), ServerConfig::default());

    h.send(want(wanted.cid, WantType::Block, false));
    h.server.peer_disconnected(h.peer);
    h.server.peer_connected(h.peer);

    let messages = h.until_sentinel(&wanted.cid).await;
    assert!(has_block(&messages, &wanted.cid));
}

#[tokio::test(start_paused = true)]
async fn tasks_wait_for_a_send_slot_when_all_workers_are_busy() {
    let blocks: Vec<Block> = (1..=3u8).map(|i| block_v1(&[i; 50_000])).collect();
    let config = ServerConfig {
        worker_count: 1,
        max_outstanding_bytes_per_peer: 0,
        target_message_size: 1,
        ..Default::default()
    };
    let mut h = start(&blocks, config);

    let mut message = BitswapMessage::new(false);
    for block in &blocks {
        message.add_entry(block.cid, 1, WantType::Block, false);
    }
    h.send(message);

    let first = h.next().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        h.sent.try_recv().is_err(),
        "one worker means one message in flight"
    );

    let _ = first.response.send(Ok(()));
    let second = h.next().await;
    let _ = second.response.send(Ok(()));
    let third = h.next().await;
    let _ = third.response.send(Ok(()));
}
