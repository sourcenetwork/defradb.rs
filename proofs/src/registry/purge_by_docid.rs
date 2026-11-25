use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "db";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // Purging documents by DocID: values, index entries, the primary
        // key, heads and DocID aliases go in one transact attempt; outside a
        // branchable collection every block only the purged documents own
        // goes too, decided by a validated owner scan; in a branchable
        // collection the history stays and no owner edge names a purged
        // document; the count is the distinct stored documents, taken at
        // commit; a sync re-storing a walked block re-runs the attempt.
        family: "Purge by DocID",
        name: "INV_OthersIntact / INV_CollectionIntact / INV_Gone / INV_NoStaleOwner / INV_Pruned / INV_Count / PROP_Commits",
        axis: Tla,
        anchor: "crates/database/db/src/collection/ops/purge.rs DB::purge_by_doc_ids_as, DB::purge_documents; crates/database/db/src/txn/attempt.rs DB::transact_db; crates/database/db/src/collection/ops/truncate/filtered.rs DB::truncate_filtered_chunk; crates/database/db/src/txn/registry/purge.rs DbTransactionRegistry::purge_by_doc_ids_as; crates/database/db/src/block/cleanup.rs delete_owned_dag_for_owners, drop_dag_owners; crates/database/db/src/docid/map.rs block_has_owners",
        model_ref: "MC_Purge_{Green_Prune,Green_Branchable,Red_Blind,Red_PruneBranchable,Red_KeepEdges,Red_KeepIndex,Red_CountRequests,Red_CountAttempt,Red_Fail,Red_PlainScan}.cfg",
        tiers: &[Boundary],
        bound_by: "",
    },
];
