use std::sync::Arc;
use std::time::{Duration, Instant};

use blockstore::{Blockstore, DefraBlockstore};
use cid::Cid;
use defra_core::{Block, CrdtDelta, DAGLink, LwwDeltaPayload};
use multihash_codetable::{Code, MultihashDigest};
use storage::RegolithStore;

use super::{
    accept_reply, collect_commit_blocks, serve_cars, BadReply, CarPeerBackoff, Entry, HeadCar,
    Outcome, MAX_CAR_REPLY_BYTES,
};
use crate::message::car_exchange::CarReply;
use crate::sync::car::encode_car;

fn run<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    workers::block_on(future).expect("block_on")
}

fn cid_of(data: &[u8]) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(data))
}

fn car(root: Cid) -> Vec<u8> {
    encode_car(&[root], &[]).unwrap()
}

fn block(heads: Vec<Cid>, links: Vec<DAGLink>, signature: Option<Cid>) -> Vec<u8> {
    Block::new_with_options(
        CrdtDelta::Lww(LwwDeltaPayload {
            field_name: "value".to_owned(),
            priority: 1,
            collection_version_id: "version".to_owned(),
            data: Vec::new(),
        }),
        heads,
        links,
        None,
        signature,
    )
    .to_dag_cbor()
    .unwrap()
}

#[test]
fn serve_skips_unserved_and_oversized_heads_and_stops_at_the_budget() {
    let heads: Vec<Cid> = (0u8..5).map(|i| cid_of(&[i])).collect();
    let raw: Vec<Vec<u8>> = heads.iter().map(Cid::to_bytes).collect();
    let mut asked: Vec<&[u8]> = raw.iter().map(Vec::as_slice).collect();
    asked.insert(1, b"not a cid");
    let mut built = Vec::new();
    let cars = run(serve_cars(&asked, |head| {
        built.push(head);
        let index = heads.iter().position(|h| *h == head).unwrap();
        std::future::ready(match index {
            0 => HeadCar::Car(vec![1; 10]),
            1 => HeadCar::Skip,
            2 => HeadCar::Car(vec![2; MAX_CAR_REPLY_BYTES + 1]),
            3 => HeadCar::Car(vec![3; MAX_CAR_REPLY_BYTES - 5]),
            _ => HeadCar::Car(vec![4; 1]),
        })
    }));
    // Head 0 served, the bad CID and head 1 empty, head 2 too big for any
    // reply so empty, head 3 does not fit what is left so the reply ends.
    assert_eq!(cars.len(), 4);
    assert_eq!(cars[0], vec![1; 10]);
    assert!(cars[1].is_empty() && cars[2].is_empty() && cars[3].is_empty());
    // Head 4 was never built: the reply ended before it.
    assert!(!built.contains(&heads[4]));
}

#[test]
fn accept_matches_entries_to_the_asked_prefix() {
    let heads = [cid_of(b"a"), cid_of(b"b"), cid_of(b"c")];
    let reply = CarReply::new("id", vec![car(heads[0]), car(heads[2])]);
    let entries = accept_reply(&heads, &reply).unwrap();
    // The second entry is rooted at another head: not served. The third head
    // is past the end and is not answered.
    assert_eq!(
        entries,
        vec![Entry::Fetched(car(heads[0])), Entry::NotServed]
    );
}

#[test]
fn accept_refuses_error_empty_and_too_long_replies() {
    let heads = [cid_of(b"a")];
    let error = CarReply::error("id", "boom");
    assert_eq!(
        accept_reply(&heads, &error),
        Err(BadReply::Error("boom".into()))
    );
    let empty = CarReply::new("id", Vec::new());
    assert_eq!(accept_reply(&heads, &empty), Err(BadReply::Empty));
    let long = CarReply::new("id", vec![car(heads[0]), car(heads[0])]);
    assert_eq!(
        accept_reply(&heads, &long),
        Err(BadReply::TooLong { got: 2, asked: 1 })
    );
    let unserved = CarReply::new("id", vec![Vec::new()]);
    assert_eq!(accept_reply(&heads, &unserved), Ok(vec![Entry::NotServed]));
}

#[test]
fn commit_blocks_follow_links_and_signature_but_not_heads() {
    run(async {
        let store = DefraBlockstore::new(Arc::new(RegolithStore::in_memory().unwrap()), true);
        let put = |data: Vec<u8>| {
            let store = &store;
            async move {
                let cid = cid_of(&data);
                store.put(&cid, &data).await.unwrap();
                cid
            }
        };
        let previous = put(block(Vec::new(), Vec::new(), None)).await;
        let field = put(block(vec![previous], Vec::new(), None)).await;
        let signature = put(b"signature".to_vec()).await;
        let head = put(block(
            vec![previous],
            vec![DAGLink::new("name", field)],
            Some(signature),
        ))
        .await;
        let get = |cid: Cid| {
            let store = &store;
            async move { Ok(store.get(&cid).await.unwrap()) }
        };
        let blocks = collect_commit_blocks(head, get).await.unwrap().unwrap();
        let cids: Vec<Cid> = blocks.iter().map(|(cid, _)| *cid).collect();
        assert_eq!(cids, vec![head, field, signature]);
        assert!(collect_commit_blocks(cid_of(b"absent"), get)
            .await
            .unwrap()
            .is_none());
        assert!(collect_commit_blocks(signature, get)
            .await
            .unwrap()
            .is_none());
        // A head linking a block this node does not hold gets no CAR.
        let partial = put(block(
            Vec::new(),
            vec![DAGLink::new("name", cid_of(b"missing"))],
            None,
        ))
        .await;
        assert!(collect_commit_blocks(partial, get).await.unwrap().is_none());
    });
}

#[test]
fn backoff_passes_over_only_after_repeated_faults() {
    let backoff = CarPeerBackoff::default();
    let now = Instant::now();
    backoff.record("peer", Outcome::Cancelled, now);
    assert!(!backoff.passed_over("peer", now));
    backoff.record("peer", Outcome::Fault, now);
    backoff.record("peer", Outcome::Fault, now);
    assert!(!backoff.passed_over("peer", now));
    backoff.record("peer", Outcome::Fault, now);
    assert!(backoff.passed_over("peer", now));
    // The window is 1s after the third fault; once it ends the peer is asked.
    assert!(!backoff.passed_over("peer", now + Duration::from_secs(2)));
    for _ in 0..3 {
        backoff.record("peer", Outcome::Fault, now);
    }
    backoff.record("peer", Outcome::Answered, now);
    assert!(!backoff.passed_over("peer", now));
}

#[test]
fn a_creator_must_be_the_signed_author() {
    use super::authenticated_creator;
    assert_eq!(
        authenticated_creator("12D3KooWA", Some("12D3KooWA")),
        "12D3KooWA"
    );
    // A forged creator is replaced by the peer that signed the message.
    assert_eq!(
        authenticated_creator("12D3KooWVictim", Some("12D3KooWA")),
        "12D3KooWA"
    );
    // An unsigned message names no creator at all.
    assert_eq!(authenticated_creator("12D3KooWVictim", None), "");
}
