use std::time::Duration;

use bitswap::client::{Client, KeepAlive, Outgoing, REQUEST_TIMEOUT};
use bitswap::{BitswapMessage, Block, Entry, ProtocolId, WantType};
use libp2p::PeerId;
use tokio::sync::mpsc::error::TryRecvError;

mod common;
use common::block_v1;

fn peer() -> PeerId {
    PeerId::random()
}

fn flush(client: &mut Client) -> Vec<Outgoing> {
    let sent = client.take_ready(|_| Some(ProtocolId::Bitswap120));
    for out in &sent {
        client.send_done(out.peer, &out.requests, true);
    }
    sent
}

fn entries(sent: &[Outgoing], to: PeerId) -> Vec<Entry> {
    let mut found: Vec<Entry> = sent
        .iter()
        .filter(|out| out.peer == to)
        .flat_map(|out| out.message.wantlist().cloned())
        .collect();
    found.sort_by_key(|entry| entry.cid.to_bytes());
    found
}

fn one_message_to(sent: &[Outgoing], to: PeerId) -> usize {
    sent.iter().filter(|out| out.peer == to).count()
}

fn have(from: &Block) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_have(from.cid);
    message
}

fn dont_have(of: &Block) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_dont_have(of.cid);
    message
}

fn with_block(block: &Block) -> BitswapMessage {
    let mut message = BitswapMessage::new(false);
    message.add_block(block.clone());
    message
}

fn closed(rx: &mut tokio::sync::mpsc::Receiver<Block>) -> bool {
    matches!(rx.try_recv(), Err(TryRecvError::Disconnected))
}

#[tokio::test]
async fn first_provider_gets_block_others_have_one_message_per_peer() {
    let blocks = [block_v1(b"one"), block_v1(b"two"), block_v1(b"three")];
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let cids = blocks.iter().map(|b| b.cid).collect();
    client.fetch(cids, vec![p0, p1]);

    let sent = flush(&mut client);
    assert_eq!(sent.len(), 2);
    assert_eq!(one_message_to(&sent, p0), 1);
    assert_eq!(one_message_to(&sent, p1), 1);
    for entry in entries(&sent, p0) {
        assert_eq!(entry.want_type, WantType::Block);
        assert!(entry.send_dont_have && !entry.cancel);
        assert_eq!(entry.priority, 1);
    }
    assert_eq!(entries(&sent, p0).len(), 3);
    for entry in entries(&sent, p1) {
        assert_eq!(entry.want_type, WantType::Have);
        assert!(entry.send_dont_have);
        assert_eq!(entry.priority, 1);
    }
    assert_eq!(entries(&sent, p1).len(), 3);
}

#[tokio::test]
async fn have_then_block_request_to_that_peer() {
    let block = block_v1(b"fallback");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    flush(&mut client);

    client.on_message(&p0, &dont_have(&block));
    assert!(flush(&mut client).is_empty());
    client.on_message(&p1, &have(&block));
    let sent = flush(&mut client);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].peer, p1);
    let wants = entries(&sent, p1);
    assert_eq!(wants.len(), 1);
    assert_eq!(wants[0].want_type, WantType::Block);

    client.on_message(&p1, &with_block(&block));
    assert_eq!(rx.recv().await.map(|b| b.cid), Some(block.cid));
    assert!(rx.recv().await.is_none());
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test]
async fn all_dont_have_completes_with_zero_blocks() {
    let block = block_v1(b"nobody");
    let providers = [peer(), peer(), peer()];
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], providers.to_vec());
    flush(&mut client);
    for provider in providers {
        client.on_message(&provider, &dont_have(&block));
    }
    assert!(rx.recv().await.is_none());
    assert!(flush(&mut client).is_empty());
    assert!(client.is_idle());
}

#[tokio::test]
async fn block_from_another_peer_completes_and_cancels_the_rest() {
    let block = block_v1(b"elsewhere");
    let (p0, p1, p2) = (peer(), peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1, p2]);
    flush(&mut client);

    client.on_message(&p1, &with_block(&block));
    assert_eq!(rx.recv().await.map(|b| b.cid), Some(block.cid));
    assert!(rx.recv().await.is_none());

    let sent = flush(&mut client);
    assert_eq!(one_message_to(&sent, p0), 1);
    assert_eq!(one_message_to(&sent, p2), 1);
    assert_eq!(one_message_to(&sent, p1), 0);
    for to in [p0, p2] {
        let cancels = entries(&sent, to);
        assert_eq!(cancels.len(), 1);
        assert!(cancels[0].cancel);
    }
    assert!(client.is_idle());
}

#[tokio::test]
async fn block_in_reply_to_a_want_have_completes() {
    let block = block_v1(b"small");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    flush(&mut client);
    client.on_message(&p1, &with_block(&block));
    assert_eq!(rx.recv().await.map(|b| b.cid), Some(block.cid));
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn cancel_cancels_only_peers_with_outstanding_requests() {
    let block = block_v1(b"cancelled");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (id, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    flush(&mut client);
    client.on_message(&p1, &dont_have(&block));

    assert!(client.cancel(id));
    assert!(!client.cancel(id));
    assert!(closed(&mut rx));
    let sent = flush(&mut client);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].peer, p0);
    assert!(entries(&sent, p0)[0].cancel);
    assert!(client.is_idle());
}

#[tokio::test(start_paused = true)]
async fn timeout_counts_as_no() {
    let block = block_v1(b"slow");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    flush(&mut client);

    tokio::time::advance(REQUEST_TIMEOUT - Duration::from_millis(1)).await;
    client.expire();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    tokio::time::advance(Duration::from_millis(2)).await;
    client.expire();
    assert!(rx.recv().await.is_none());
    assert!(client.next_deadline().is_none());
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test]
async fn disconnect_counts_as_no_and_unprotects() {
    let block = block_v1(b"gone");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    flush(&mut client);
    assert_eq!(
        client.take_keep_alive(),
        [KeepAlive::Protect(p0), KeepAlive::Protect(p1)]
    );

    client.on_peer_disconnected(&p0);
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(client.take_keep_alive(), [KeepAlive::Unprotect(p0)]);

    client.on_peer_disconnected(&p1);
    assert!(rx.recv().await.is_none());
    assert_eq!(client.take_keep_alive(), [KeepAlive::Unprotect(p1)]);
    assert!(flush(&mut client).is_empty());
    assert!(client.is_idle());
}

#[tokio::test]
async fn send_failure_counts_as_no_for_every_carried_request() {
    let blocks = [block_v1(b"a"), block_v1(b"b")];
    let p0 = peer();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(blocks.iter().map(|b| b.cid).collect(), vec![p0]);
    let sent = client.take_ready(|_| None);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].requests.len(), 2);
    client.send_done(p0, &sent[0].requests, false);
    assert!(rx.recv().await.is_none());
    assert!(flush(&mut client).is_empty());
    assert!(client.is_idle());
}

#[tokio::test]
async fn one_send_in_flight_per_peer_and_later_wants_coalesce() {
    let (first, second) = (block_v1(b"first"), block_v1(b"second"));
    let p0 = peer();
    let mut client = Client::new();
    client.fetch(vec![first.cid], vec![p0]);
    let sent = client.take_ready(|_| None);
    client.fetch(vec![second.cid], vec![p0]);
    assert!(client.take_ready(|_| None).is_empty());

    client.send_done(p0, &sent[0].requests, true);
    let sent = client.take_ready(|_| None);
    assert_eq!(sent.len(), 1);
    assert_eq!(entries(&sent, p0)[0].cid, second.cid);
}

#[tokio::test]
async fn two_fetches_sharing_a_cid_both_receive_it() {
    let block = block_v1(b"shared");
    let p0 = peer();
    let mut client = Client::new();
    let (_, mut rx_a) = client.fetch(vec![block.cid], vec![p0]);
    let (_, mut rx_b) = client.fetch(vec![block.cid], vec![p0]);
    let sent = flush(&mut client);
    assert_eq!(sent.len(), 1);
    assert_eq!(entries(&sent, p0).len(), 1);

    client.on_message(&p0, &with_block(&block));
    for rx in [&mut rx_a, &mut rx_b] {
        assert_eq!(rx.recv().await.map(|b| b.cid), Some(block.cid));
        assert!(rx.recv().await.is_none());
    }
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test]
async fn cancelling_one_of_two_fetches_keeps_the_get_alive() {
    let block = block_v1(b"kept");
    let p0 = peer();
    let mut client = Client::new();
    let (id_a, mut rx_a) = client.fetch(vec![block.cid], vec![p0]);
    let (_, mut rx_b) = client.fetch(vec![block.cid], vec![p0]);
    flush(&mut client);
    assert!(client.cancel(id_a));
    assert!(closed(&mut rx_a));
    assert!(flush(&mut client).is_empty());

    client.on_message(&p0, &with_block(&block));
    assert_eq!(rx_b.recv().await.map(|b| b.cid), Some(block.cid));
}

#[tokio::test]
async fn have_is_a_block_want_for_a_peer_without_have_support() {
    let block = block_v1(b"legacy");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    client.fetch(vec![block.cid], vec![p0, p1]);
    let sent = client.take_ready(|p| (*p == p1).then_some(ProtocolId::Bitswap110));
    assert_eq!(entries(&sent, p1)[0].want_type, WantType::Block);
}

#[tokio::test]
async fn duplicates_are_dropped_and_empty_inputs_complete_at_once() {
    let block = block_v1(b"dup");
    let p0 = peer();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid, block.cid], vec![p0, p0]);
    let sent = flush(&mut client);
    assert_eq!(sent.len(), 1);
    assert_eq!(entries(&sent, p0).len(), 1);
    client.on_message(&p0, &with_block(&block));
    assert!(rx.recv().await.is_some());
    assert!(rx.recv().await.is_none());

    for (cids, providers) in [(vec![block.cid], vec![]), (vec![], vec![p0])] {
        let (_, mut rx) = client.fetch(cids, providers);
        assert!(rx.recv().await.is_none());
    }
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test]
async fn a_block_without_an_active_get_is_ignored() {
    let block = block_v1(b"unwanted");
    let mut client = Client::new();
    client.on_message(&peer(), &with_block(&block));
    assert!(client.is_idle());
}

#[tokio::test(start_paused = true)]
async fn deadline_starts_when_the_send_completes() {
    let block = block_v1(b"dial");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    let sent = client.take_ready(|_| None);
    assert!(client.next_deadline().is_none());

    tokio::time::advance(Duration::from_secs(15)).await;
    client.expire();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    for out in &sent {
        client.send_done(out.peer, &out.requests, true);
    }
    assert!(client.next_deadline().is_some());
    tokio::time::advance(REQUEST_TIMEOUT - Duration::from_millis(1)).await;
    client.expire();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    tokio::time::advance(Duration::from_millis(2)).await;
    client.expire();
    assert!(rx.recv().await.is_none());
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test(start_paused = true)]
async fn send_failure_is_a_no_without_waiting_for_a_deadline() {
    let block = block_v1(b"refused");
    let p0 = peer();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0]);
    let sent = client.take_ready(|_| None);
    client.send_done(p0, &sent[0].requests, false);
    assert!(rx.recv().await.is_none());
    assert!(client.next_deadline().is_none());
    assert!(client.is_idle());
}

#[tokio::test(start_paused = true)]
async fn response_before_send_completion_leaves_no_deadline() {
    let block = block_v1(b"early");
    let p0 = peer();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(vec![block.cid], vec![p0]);
    let sent = client.take_ready(|_| None);

    client.on_message(&p0, &with_block(&block));
    assert_eq!(rx.recv().await.map(|b| b.cid), Some(block.cid));
    client.send_done(p0, &sent[0].requests, true);
    assert!(client.next_deadline().is_none());
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test(start_paused = true)]
async fn a_late_send_result_does_not_arm_a_superseded_request() {
    let block = block_v1(b"superseded");
    let (p0, p1) = (peer(), peer());
    let mut client = Client::new();
    let (id, mut rx) = client.fetch(vec![block.cid], vec![p0, p1]);
    let sent = client.take_ready(|_| None);
    assert!(client.cancel(id));
    assert!(closed(&mut rx));
    for out in &sent {
        client.send_done(out.peer, &out.requests, true);
    }
    assert!(client.next_deadline().is_none());
    flush(&mut client);
    assert!(client.is_idle());
}
