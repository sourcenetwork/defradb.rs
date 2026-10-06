use std::collections::BTreeSet;
use std::time::Duration;

use bitswap::client::{Client, Outgoing, MAX_OUTSTANDING_WANTS_PER_PEER, REQUEST_TIMEOUT};
use bitswap::{BitswapMessage, Block, ProtocolId};
use cid::Cid;
use libp2p::PeerId;
use tokio::sync::mpsc::error::TryRecvError;

mod common;
use common::block_v1;

const CAP: usize = MAX_OUTSTANDING_WANTS_PER_PEER;

fn blocks(prefix: &str, n: usize) -> Vec<Block> {
    (0..n)
        .map(|i| block_v1(format!("{prefix} {i}").as_bytes()))
        .collect()
}

fn cids(blocks: &[Block]) -> Vec<Cid> {
    blocks.iter().map(|b| b.cid).collect()
}

fn flush(client: &mut Client) -> Vec<Outgoing> {
    let sent = client.take_ready(|_| Some(ProtocolId::Bitswap120));
    for out in &sent {
        client.send_done(out.peer, &out.requests, true);
    }
    sent
}

fn wants(sent: &[Outgoing], to: PeerId) -> BTreeSet<Cid> {
    sent.iter()
        .filter(|out| out.peer == to)
        .flat_map(|out| out.message.wantlist())
        .filter(|entry| !entry.cancel)
        .map(|entry| entry.cid)
        .collect()
}

fn cancels(sent: &[Outgoing], to: PeerId) -> Vec<Cid> {
    sent.iter()
        .filter(|out| out.peer == to)
        .flat_map(|out| out.message.wantlist())
        .filter(|entry| entry.cancel)
        .map(|entry| entry.cid)
        .collect()
}

fn serve(client: &mut Client, from: &PeerId, served: &[Block]) {
    for block in served {
        let mut message = BitswapMessage::new(false);
        message.add_block(block.clone());
        client.on_message(from, &message);
    }
}

fn presence(client: &mut Client, from: &PeerId, of: &Block, have: bool) {
    let mut message = BitswapMessage::new(false);
    if have {
        message.add_have(of.cid);
    } else {
        message.add_dont_have(of.cid);
    }
    client.on_message(from, &message);
}

#[tokio::test]
async fn a_large_fetch_issues_in_fifo_windows() {
    let all = blocks("big", 2048);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(cids(&all), vec![p0]);

    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p0), cids(&all[..CAP]).into_iter().collect());
    assert!(flush(&mut client).is_empty());

    let mut issued = CAP;
    while issued < all.len() {
        let k = 7.min(all.len() - issued);
        serve(&mut client, &p0, &all[issued - CAP..issued - CAP + k]);
        let sent = flush(&mut client);
        assert_eq!(
            wants(&sent, p0),
            cids(&all[issued..issued + k]).into_iter().collect()
        );
        assert!(cancels(&sent, p0).is_empty());
        issued += k;
    }
    serve(&mut client, &p0, &all[all.len() - CAP..]);
    serve(&mut client, &p0, &all[..all.len() - CAP]);
    let mut got = 0;
    while rx.recv().await.is_some() {
        got += 1;
    }
    assert_eq!(got, all.len());
    flush(&mut client);
    assert!(client.is_idle());
}

#[tokio::test]
async fn the_window_is_shared_by_concurrent_fetches() {
    let (a, b) = (blocks("a", 400), blocks("b", 400));
    let p0 = PeerId::random();
    let mut client = Client::new();
    let _rx_a = client.fetch(cids(&a), vec![p0]);
    let _rx_b = client.fetch(cids(&b), vec![p0]);

    let sent = flush(&mut client);
    let first = wants(&sent, p0);
    assert_eq!(first.len(), CAP);
    assert!(cids(&a).iter().all(|cid| first.contains(cid)));

    serve(&mut client, &p0, &a[..100]);
    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p0).len(), 100);
    serve(&mut client, &p0, &a[100..]);
    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p0).len(), 800 - CAP - 100);
}

#[tokio::test]
async fn dont_have_cancels_once_and_frees_the_slot() {
    let all = blocks("dh", CAP + 1);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(cids(&all), vec![p0]);
    flush(&mut client);

    presence(&mut client, &p0, &all[0], false);
    let sent = flush(&mut client);
    assert_eq!(cancels(&sent, p0), [all[0].cid]);
    assert_eq!(wants(&sent, p0), BTreeSet::from([all[CAP].cid]));
    assert!(flush(&mut client).is_empty());

    presence(&mut client, &p0, &all[0], false);
    assert!(flush(&mut client).is_empty());
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn block_and_have_free_the_slot_without_a_cancel() {
    let all = blocks("bh", CAP + 2);
    let (p0, p1) = (PeerId::random(), PeerId::random());
    let mut client = Client::new();
    let _rx = client.fetch(cids(&all), vec![p0, p1]);
    flush(&mut client);

    serve(&mut client, &p0, &all[..1]);
    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p0), BTreeSet::from([all[CAP].cid]));
    assert!(cancels(&sent, p0).is_empty());
    assert_eq!(cancels(&sent, p1), [all[0].cid]);
    assert_eq!(wants(&sent, p1), BTreeSet::from([all[CAP].cid]));

    presence(&mut client, &p1, &all[1], true);
    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p1), BTreeSet::from([all[CAP + 1].cid]));
    assert!(cancels(&sent, p1).is_empty());
}

#[tokio::test(start_paused = true)]
async fn timeout_cancels_and_frees_the_slots() {
    let all = blocks("to", CAP + 1);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(cids(&all), vec![p0]);
    flush(&mut client);

    tokio::time::advance(REQUEST_TIMEOUT + Duration::from_millis(1)).await;
    client.expire();
    let sent = flush(&mut client);
    assert_eq!(cancels(&sent, p0).len(), CAP);
    assert_eq!(wants(&sent, p0), BTreeSet::from([all[CAP].cid]));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn send_failure_frees_the_slots_without_a_cancel() {
    let all = blocks("sf", CAP + 1);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let _rx = client.fetch(cids(&all), vec![p0]);
    let sent = client.take_ready(|_| None);
    assert_eq!(sent[0].requests.len(), CAP);
    client.send_done(p0, &sent[0].requests, false);

    let sent = flush(&mut client);
    assert_eq!(wants(&sent, p0), BTreeSet::from([all[CAP].cid]));
    assert!(cancels(&sent, p0).is_empty());
}

#[tokio::test]
async fn disconnect_frees_the_window_without_a_cancel() {
    let all = blocks("dc", CAP + 1);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, mut rx) = client.fetch(cids(&all), vec![p0]);
    flush(&mut client);

    client.on_peer_disconnected(&p0);
    assert!(flush(&mut client).is_empty());
    assert!(rx.recv().await.is_none());
    assert!(client.is_idle());
}

#[tokio::test]
async fn a_dropped_receiver_stops_the_backlog_and_cancels_outstanding_wants() {
    let all = blocks("drop", CAP + 88);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, rx) = client.fetch(cids(&all), vec![p0]);
    flush(&mut client);
    drop(rx);

    presence(&mut client, &p0, &all[0], false);
    let sent = flush(&mut client);
    assert!(wants(&sent, p0).is_empty());
    assert_eq!(
        cancels(&sent, p0).into_iter().collect::<BTreeSet<_>>(),
        cids(&all[..CAP]).into_iter().collect()
    );
    assert!(client.is_idle());
}

#[tokio::test]
async fn a_block_for_a_dropped_receiver_cancels_the_rest() {
    let all = blocks("drop-block", CAP + 8);
    let p0 = PeerId::random();
    let mut client = Client::new();
    let (_, rx) = client.fetch(cids(&all), vec![p0]);
    flush(&mut client);
    drop(rx);

    serve(&mut client, &p0, &all[..1]);
    let sent = flush(&mut client);
    assert!(wants(&sent, p0).is_empty());
    assert_eq!(cancels(&sent, p0).len(), CAP - 1);
    assert!(client.is_idle());
}
