//! Adding back-edges to an encoded DiskANN record in place writes the bytes
//! that decoding it, adding each edge and encoding it again would.

use db::index::vector::engine::diskann::codec::{self, Record};
use db::index::vector::store::NodeId;
use proptest::prelude::*;

type Edges = Vec<(NodeId, Vec<u8>)>;

/// The old loop in `add_back_edges`, on a decoded record.
fn add_decoded(record: &mut Record, target: NodeId, edges: &Edges) {
    for (id, code) in edges {
        if *id == target {
            continue;
        }
        match record.neighbors.iter_mut().find(|(n, _)| n == id) {
            Some(edge) => edge.1 = code.clone(),
            None => record.neighbors.push((*id, code.clone())),
        }
    }
}

fn edges(m: usize, max: usize) -> impl Strategy<Value = Edges> {
    prop::collection::vec(
        (0u64..12, prop::collection::vec(any::<u8>(), m)).prop_map(|(id, code)| (NodeId(id), code)),
        0..max,
    )
}

fn case() -> impl Strategy<Value = (usize, Vec<u8>, Edges, u64, Edges)> {
    (0usize..5).prop_flat_map(|m| {
        (
            Just(m),
            prop::collection::vec(any::<u8>(), m),
            edges(m, 10),
            0u64..12,
            edges(m, 10),
        )
    })
}

proptest! {
    #[test]
    fn adding_edges_in_place_writes_what_decoding_and_encoding_would(
        (m, own, existing, target, added) in case()
    ) {
        let target = NodeId(target);
        let mut record = Record { code: own, neighbors: Vec::new() };
        add_decoded(&mut record, target, &existing);
        let mut bytes = codec::encode_record(&record);

        let count = codec::add_edges(&mut bytes, m, target, &added).unwrap();
        add_decoded(&mut record, target, &added);

        prop_assert_eq!(&bytes, &codec::encode_record(&record));
        prop_assert_eq!(count, record.neighbors.len());
    }
}

#[test]
fn a_corrupt_record_is_an_error() {
    let mut bytes = codec::encode_record(&Record {
        code: vec![1, 2],
        neighbors: vec![(NodeId(1), vec![3, 4])],
    });
    bytes.pop();
    assert!(codec::add_edges(&mut bytes, 2, NodeId(9), &[(NodeId(2), vec![5, 6])]).is_err());
}

#[test]
fn a_code_of_the_wrong_length_is_an_error() {
    let mut bytes = codec::encode_record(&Record {
        code: vec![1, 2],
        neighbors: Vec::new(),
    });
    assert!(codec::add_edges(&mut bytes, 2, NodeId(9), &[(NodeId(2), vec![5])]).is_err());
}
