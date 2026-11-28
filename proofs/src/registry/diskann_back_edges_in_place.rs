use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index (vector)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DiskANN back-edges in place",
        name: "in_place_eq_decoded — adding back-edges to an encoded record in place leaves the entries and count that decoding, adding and encoding would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs add_edges; crates/database/db/src/index/vector/engine/diskann/back_edges.rs add_back_edges",
        model_ref: "BackEdgesInPlace.in_place_eq_decoded / stale_search_duplicates_an_edge",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_back_edges.rs adding_edges_in_place_writes_what_decoding_and_encoding_would",
    },
    Property {
        family: "DiskANN back-edges in place",
        name: "resident_eq_decoded — adding back-edges to an ids-only record without looking up neighbour codes writes the ids that looking them up, adding, pruning and dropping the codes would",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs add_ids; crates/database/db/src/index/vector/engine/diskann/back_edges.rs add_resident_back_edges",
        model_ref: "ResidentBackEdges.resident_eq_decoded / in_place_ids_eq_added / skipping_the_filter_keeps_a_consolidated_neighbour",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_resident_back_edges.rs resident_records_are_the_records_with_codes_left_out, a_back_edge_drops_a_neighbour_consolidated_away; crates/database/db/tests/vector/diskann_back_edges.rs adding_ids_in_place_writes_what_adding_edges_and_dropping_codes_would",
    },
];
