use std::time::{Duration, Instant};

use bytes::Bytes;
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

use super::{
    encode_batch, InlineCars, ShinzoBatcher, BATCH_FLUSH_INTERVAL, BATCH_MAX_DOCS, INLINE_CAR_TTL,
    MAX_BATCH_MESSAGE_BYTES,
};
use crate::message::{DocumentInfo, PushLogBroadcast};
use crate::sync::car::encode_car;

fn cid_of(data: &[u8]) -> Cid {
    Cid::new_v1(0x71, Code::Sha2_256.digest(data))
}

fn car(root: Cid) -> Bytes {
    Bytes::from(encode_car(&[root], &[]).unwrap())
}

fn document(i: usize, size: usize) -> DocumentInfo {
    DocumentInfo {
        doc_id: format!("doc-{i}"),
        cid: Bytes::from(cid_of(&i.to_le_bytes()).to_bytes()),
        block: Bytes::from(vec![0xa1; size]),
        car: Bytes::new(),
    }
}

#[test]
fn inline_car_is_kept_for_its_own_head_and_used_once() {
    let mut cars = InlineCars::default();
    let (head, other) = (cid_of(b"head"), cid_of(b"other"));
    let now = Instant::now();
    assert!(
        !cars.offer(head, car(other), now),
        "a CAR rooted elsewhere is refused"
    );
    assert!(cars.offer(head, car(head), now));
    assert_eq!(cars.take(&head), Some(car(head)));
    assert_eq!(cars.take(&head), None);
}

#[test]
fn inline_car_expires_after_its_ttl() {
    let mut cars = InlineCars::default();
    let (first, second) = (cid_of(b"first"), cid_of(b"second"));
    let now = Instant::now();
    assert!(cars.offer(first, car(first), now));
    assert!(cars.offer(second, car(second), now + INLINE_CAR_TTL));
    assert_eq!(cars.take(&first), None);
    assert!(cars.take(&second).is_some());
}

#[test]
fn a_batch_is_published_full_or_after_the_flush_interval() {
    let mut batcher = ShinzoBatcher::default();
    let now = Instant::now();
    for i in 0..BATCH_MAX_DOCS - 1 {
        assert!(batcher.add("col", document(i, 1), now).is_none());
    }
    let full = batcher.add("col", document(99, 1), now).unwrap();
    assert_eq!(full.len(), BATCH_MAX_DOCS);
    assert!(batcher.is_empty());

    batcher.add("col", document(1, 1), now);
    assert!(batcher.due(now + Duration::from_millis(10)).is_empty());
    let due = batcher.due(now + BATCH_FLUSH_INTERVAL);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].0, "col");
    assert!(batcher.is_empty());
}

#[test]
fn an_encoded_batch_reads_back_as_go_reads_it() {
    let documents: Vec<_> = (0..3).map(|i| document(i, 4)).collect();
    let messages = encode_batch("col", "12D3KooWRust", documents.clone()).unwrap();
    assert_eq!(messages.len(), 1);
    let decoded = PushLogBroadcast::decode_gossip_documents(&messages[0])
        .unwrap()
        .unwrap();
    assert_eq!(decoded.len(), 3);
    for ((broadcast, _), document) in decoded.iter().zip(&documents) {
        assert_eq!(broadcast.doc_id, document.doc_id);
        assert_eq!(broadcast.cid, document.cid);
        assert_eq!(broadcast.collection_id, "col");
        assert_eq!(broadcast.creator, "12D3KooWRust");
    }
}

#[test]
fn an_oversized_batch_is_split_in_halves() {
    let size = MAX_BATCH_MESSAGE_BYTES / 3;
    let documents: Vec<_> = (0..4).map(|i| document(i, size)).collect();
    let messages = encode_batch("col", "creator", documents).unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|m| m.len() < MAX_BATCH_MESSAGE_BYTES));
}
