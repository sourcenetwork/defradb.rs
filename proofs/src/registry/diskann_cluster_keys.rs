use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db-index (vector)";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        family: "DiskANN cluster keys",
        name: "INV_OneRecord / INV_FindsRecord — a node has one record, at the key its code names, and a search finds it whatever generation the shared code cache holds",
        axis: Tla,
        anchor: "crates/database/db/src/index/vector/engine/diskann/record.rs locate, record, choose_cluster; crates/database/db/src/index/vector/engine/diskann/graph.rs link; crates/database/db/src/index/vector/engine/diskann/merge.rs consolidate",
        model_ref: "MC_DiskAnnClusterKeys_Green.cfg (GREEN) / MC_DiskAnnClusterKeys_Red_NoFallback.cfg / MC_DiskAnnClusterKeys_Red_MoveCluster.cfg (RED)",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_clusters.rs a_search_finds_records_whatever_cluster_the_cache_names, a_node_linked_again_keeps_its_cluster, an_index_written_before_clusters_keeps_working",
    },
    Property {
        family: "DiskANN cluster keys",
        name: "graph_id_key / graph_cluster_key / decode_encode_code / slot_round_trip — record keys and code values decode back to their id and cluster, in either layout, and no tag is the no-cluster slot marker",
        axis: Lean,
        anchor: "crates/database/db/src/index/vector/engine/diskann/codec.rs graph_key, graph_id, encode_code, decode_code, cluster_tag; crates/database/db/src/index/vector/engine/diskann/cache.rs Codes::insert, Codes::get",
        model_ref: "DiskAnnClusterKeys.graph_id_key / graph_cluster_key / graph_key_injective / decode_encode_code / tag_ne_none / slot_round_trip",
        tiers: &[Contract],
        bound_by: "crates/database/db/tests/vector/diskann_clusters.rs graph_keys_and_codes_round_trip; crates/database/db/tests/vector/diskann_resident.rs every_loaded_code_reads_back",
    },
];
