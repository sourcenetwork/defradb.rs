use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DiskANN back-edges in place",
        name: "in_place_eq_decoded — adding back-edges to an encoded record in place leaves the entries and count that decoding, adding and encoding would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs add_edges; crates/database/db/src/index/vector/engine/diskann/graph.rs add_back_edges",
        model_ref: "BackEdgesInPlace.in_place_eq_decoded / stale_search_duplicates_an_edge",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_back_edges.rs adding_edges_in_place_writes_what_decoding_and_encoding_would",
    },
];
