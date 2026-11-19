use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bitswap::server::ServerConfig;
use bitswap::{BitswapMessage, Block, WantType};
use cid::Cid;
use libp2p::PeerId;

mod common;
mod server_harness;
use common::{block_v1, cid_v1};
use server_harness::{cancel, has_block, has_dont_have, has_have, start, want, Sent};

#[test]
fn want_have_of_small_block_is_answered_with_the_block() {
    edge::testing::paused(|edge| async move {
        let small = block_v1(&[1; 100]);
        let mut h = start(std::slice::from_ref(&small), ServerConfig::default(), &edge);

        h.send(want(small.cid, WantType::Have, false));
        let messages = h.until_sentinel(&small.cid).await;

        assert!(!has_have(&messages, &small.cid));
        assert_eq!(messages[0].pending_bytes(), 0);
    })
}

#[test]
fn want_have_of_block_over_replace_size_is_answered_with_have() {
    edge::testing::paused(|edge| async move {
        let big = block_v1(&[2; 1025]);
        let sentinel = block_v1(b"sentinel");
        let mut h = start(
            &[big.clone(), sentinel.clone()],
            ServerConfig::default(),
            &edge,
        );

        h.send(want(big.cid, WantType::Have, false));
        let first = h.next().await;
        let _ = first.response.send(Ok(()));
        assert!(first.message.haves().any(|c| *c == big.cid));
        assert_eq!(first.message.blocks_len(), 0);

        h.send(want(big.cid, WantType::Block, false));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;
        assert!(has_block(&messages, &big.cid));
    })
}

#[test]
fn missing_want_block_with_send_dont_have_is_answered_with_dont_have() {
    edge::testing::paused(|edge| async move {
        let missing = cid_v1(b"missing");
        let mut h = start(&[], ServerConfig::default(), &edge);

        h.send(want(missing, WantType::Block, true));
        let sent = h.next().await;
        let _ = sent.response.send(Ok(()));

        assert!(sent.message.dont_haves().any(|c| *c == missing));
        assert_eq!(sent.message.blocks_len(), 0);
    })
}

#[test]
fn missing_want_without_send_dont_have_is_not_answered() {
    edge::testing::paused(|edge| async move {
        let missing = cid_v1(b"missing");
        let sentinel = block_v1(b"sentinel");
        let mut h = start(
            std::slice::from_ref(&sentinel),
            ServerConfig::default(),
            &edge,
        );

        h.send(want(missing, WantType::Block, false));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;

        assert!(!has_dont_have(&messages, &missing));
    })
}

#[test]
fn server_without_dont_haves_never_sends_them() {
    edge::testing::paused(|edge| async move {
        let missing = cid_v1(b"missing");
        let sentinel = block_v1(b"sentinel");
        let config = ServerConfig {
            send_dont_haves: false,
            ..Default::default()
        };
        let mut h = start(std::slice::from_ref(&sentinel), config, &edge);

        h.send(want(missing, WantType::Block, true));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;

        assert!(!has_dont_have(&messages, &missing));
    })
}

fn deny(denied: Cid) -> Box<dyn bitswap::PeerBlockRequestFilter> {
    Box::new(
        move |_: &PeerId, cid: &Cid| -> Pin<Box<dyn Future<Output = bool> + Send>> {
            let allowed = *cid != denied;
            Box::pin(async move { allowed })
        },
    )
}

#[test]
fn filter_denied_want_gets_dont_have_even_when_the_block_exists() {
    edge::testing::paused(|edge| async move {
        let secret = block_v1(b"secret");
        let mut h = start(
            std::slice::from_ref(&secret),
            ServerConfig {
                peer_block_request_filter: Some(deny(secret.cid)),
                ..Default::default()
            },
            &edge,
        );

        h.send(want(secret.cid, WantType::Block, true));
        let sent = h.next().await;
        let _ = sent.response.send(Ok(()));

        assert!(sent.message.dont_haves().any(|c| *c == secret.cid));
        assert_eq!(sent.message.blocks_len(), 0);
    })
}

#[test]
fn filter_denied_want_without_send_dont_have_is_silent() {
    edge::testing::paused(|edge| async move {
        let secret = block_v1(b"secret");
        let sentinel = block_v1(b"sentinel");
        let mut h = start(
            &[secret.clone(), sentinel.clone()],
            ServerConfig {
                peer_block_request_filter: Some(deny(secret.cid)),
                ..Default::default()
            },
            &edge,
        );

        h.send(want(secret.cid, WantType::Block, false));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;

        assert!(!has_block(&messages, &secret.cid));
        assert!(!has_dont_have(&messages, &secret.cid));
    })
}

#[test]
fn cancel_before_the_tick_removes_the_task() {
    edge::testing::paused(|edge| async move {
        let wanted = block_v1(b"wanted");
        let sentinel = block_v1(b"sentinel");
        let mut h = start(
            &[wanted.clone(), sentinel.clone()],
            ServerConfig::default(),
            &edge,
        );

        h.send(want(wanted.cid, WantType::Block, false));
        h.send(cancel(wanted.cid));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;

        assert!(!has_block(&messages, &wanted.cid));
    })
}

#[test]
fn full_wantlist_clears_the_ledger_so_a_later_cancel_finds_nothing() {
    edge::testing::paused(|edge| async move {
        let wanted = block_v1(b"wanted");
        let sentinel = block_v1(b"sentinel");
        let mut h = start(
            &[wanted.clone(), sentinel.clone()],
            ServerConfig::default(),
            &edge,
        );

        h.send(want(wanted.cid, WantType::Block, false));
        h.send(BitswapMessage::new(true));
        h.send(cancel(wanted.cid));
        h.send(want(sentinel.cid, WantType::Block, false));
        let messages = h.until_sentinel(&sentinel.cid).await;

        assert!(has_block(&messages, &wanted.cid));
    })
}

#[test]
fn outstanding_cap_holds_back_work_until_tasks_are_done() {
    edge::testing::paused(|edge| async move {
        const SIZE: usize = 600_000;
        let blocks: Vec<Block> = (1..=3u8).map(|i| block_v1(&vec![i; SIZE])).collect();
        let mut h = start(&blocks, ServerConfig::default(), &edge);

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

        nuclei::time::sleep(Duration::from_secs(2)).await;
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
    })
}

#[test]
fn disconnect_drops_the_ledger_but_not_queued_work() {
    edge::testing::paused(|edge| async move {
        let wanted = block_v1(b"wanted");
        let mut h = start(
            std::slice::from_ref(&wanted),
            ServerConfig::default(),
            &edge,
        );

        h.send(want(wanted.cid, WantType::Block, false));
        h.server.peer_disconnected(h.peer);
        h.server.peer_connected(h.peer);

        let messages = h.until_sentinel(&wanted.cid).await;
        assert!(has_block(&messages, &wanted.cid));
    })
}

#[test]
fn tasks_wait_for_a_send_slot_when_all_workers_are_busy() {
    edge::testing::paused(|edge| async move {
        let blocks: Vec<Block> = (1..=3u8).map(|i| block_v1(&[i; 50_000])).collect();
        let config = ServerConfig {
            worker_count: 1,
            max_outstanding_bytes_per_peer: 0,
            target_message_size: 1,
            ..Default::default()
        };
        let mut h = start(&blocks, config, &edge);

        let mut message = BitswapMessage::new(false);
        for block in &blocks {
            message.add_entry(block.cid, 1, WantType::Block, false);
        }
        h.send(message);

        let first = h.next().await;
        nuclei::time::sleep(Duration::from_secs(1)).await;
        assert!(
            h.sent.try_recv().is_err(),
            "one worker means one message in flight"
        );

        let _ = first.response.send(Ok(()));
        let second = h.next().await;
        let _ = second.response.send(Ok(()));
        let third = h.next().await;
        let _ = third.response.send(Ok(()));
    })
}
