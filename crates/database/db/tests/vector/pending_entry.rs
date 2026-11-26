//! A deferred vector index's pending entry, in Go's layout
//! (`proofs/lean/VectorPending.lean`).

use db::index::vector::pending::entry::{
    decode_value, encode_value, node_from_key, node_key, PendingOp,
};
use db::index::vector::store::NodeId;

#[test]
fn an_entry_reads_back_as_written() {
    let vector = [0.5f32, -1.25, f32::MIN_POSITIVE, 3.0e38];
    let entry = decode_value(NodeId(7), &encode_value(PendingOp::Upsert, &vector)).unwrap();
    assert_eq!(
        (entry.id, entry.op, entry.vector.as_slice()),
        (NodeId(7), PendingOp::Upsert, vector.as_slice())
    );
    let entry = decode_value(NodeId(7), &encode_value(PendingOp::Delete, &[])).unwrap();
    assert_eq!((entry.op, entry.vector.len()), (PendingOp::Delete, 0));
}

/// Go's layout: one op byte, then little-endian `f32`s.
#[test]
fn the_value_is_gos_bytes() {
    assert_eq!(
        encode_value(PendingOp::Upsert, &[1.0]),
        [1, 0x00, 0x00, 0x80, 0x3f]
    );
    assert_eq!(encode_value(PendingOp::Delete, &[]), [2]);
}

#[test]
fn a_malformed_value_is_refused_with_gos_text() {
    let empty = decode_value(NodeId(3), &[]).unwrap_err();
    assert_eq!(
        empty.to_string(),
        "malformed vector pending entry. NodeID: 3, Length: 0"
    );
    let cut = decode_value(NodeId(3), &[1, 0, 0]).unwrap_err();
    assert_eq!(
        cut.to_string(),
        "malformed vector pending entry. NodeID: 3, Length: 3"
    );
    let unknown = decode_value(NodeId(3), &[9]).unwrap_err();
    assert_eq!(
        unknown.to_string(),
        "unknown vector pending entry op. NodeID: 3, Op: 9"
    );
}

/// Keys sort by node, so a scan visits entries in node order.
#[test]
fn keys_round_trip_and_sort_by_node() {
    let ids = [1u64, 7, 255, 70_000, 1 << 40];
    for id in ids {
        assert_eq!(node_from_key(&node_key(NodeId(id))).unwrap(), NodeId(id));
    }
    let keys: Vec<Vec<u8>> = ids.iter().map(|&id| node_key(NodeId(id))).collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
    assert!(node_from_key(&[]).is_err());
}
