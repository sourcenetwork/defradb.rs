use super::{Axis::*, Property, Tier::*};

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "Secondary index equal-key order",
        name: "INV_Page: an ordered scan reserves a saturated offset plus limit and the LimitNode applies the offset once",
        axis: Tla,
        anchor: "crates/engine/query/src/planner/builder/index_methods.rs order_index_scan_params",
        model_ref: "MC_OrderedIndexPage_{Green,Red_DoubleOffset}.cfg",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/index/index_offset_limit.rs",
    },
    Property {
        family: "Secondary index equal-key order",
        name: "OrderedIndexPage.saturating_budget_page: an ordered scan reserves the offset plus limit without dropping the offset twice",
        axis: Lean,
        anchor: "crates/engine/query/src/planner/builder/index_methods.rs order_index_scan_params",
        model_ref: "OrderedIndexPage (lake build)",
        tiers: &[Boundary],
        bound_by: "tools/integration-test/tests/testo/index/index_offset_limit.rs",
    },
    Property {
        family: "Secondary index equal-key order",
        name: "IndexEqualKeyOrder.exactMatch_ascending / exactMatch_creation_order / later_create_larger / docIdOrder_can_reverse_creation / inScan_scan_order_without_repeats / resolveUpTo_same_page: equal index keys come back in this node's short-ID order, which follows when each document was first stored on this node (created, or first received from a peer) rather than when its source created it, as Go returns them, paged without a reorder; an InScan lists each document once, at its first encounter; resolving only the page gives the page full resolution gives",
        axis: Lean,
        anchor: "crates/database/db/src/read/index_order.rs apply_exact_match_page, extend_unique, resolve_bound, resolve_in_order; crates/database/db/src/read/doc.rs get_by_index_scan; crates/database/db/src/read/lensed/fetcher/scan.rs get_by_index_scan_inner; crates/database/db/src/read/lensed/autocommit/scan.rs get_by_index_scan_inner; crates/database/db/src/docid/map.rs DocShortIdAllocator::next",
        model_ref: "IndexEqualKeyOrder (lake build)",
        tiers: &[Boundary],
        bound_by: "crates/database/db/tests/read/index_composite_in_order.rs partial_in_over_a_composite_index_keeps_secondary_field_order; tools/integration-test/tests/testo/encryption/peer_sec_index.rs doc_encryption_peer_if_encrypted_doc_has_indexed_field_should_index_after_decryption",
    },
];
