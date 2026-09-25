//! Installing an application's merge governance on a node.

use std::sync::Arc;

use defra_core::thread_bounds::MaybeSendSync;
use storage::corekv::Store;

use super::local_write::install_local_write_judge;
use super::validator::MergeGovernance;
use crate::database::DB;

/// Claim `governance`'s collections for its validator, and judge this node's
/// own writes by that validator from now on, whether or not a replication
/// stack ever runs. First call wins, for both.
///
/// Call it before anything can write or merge: before a network surface
/// starts, and before the first mutation. A node that installs governance
/// only through a replication stack judges nothing it writes while no
/// stack is running, which on a node started without P2P, or a browser
/// that has not joined the network yet, is every write it makes.
pub fn install_merge_governance<S, B>(
    db: &Arc<DB<S>>,
    blockstore: Arc<B>,
    governance: MergeGovernance,
    max_merge_depth: usize,
) where
    S: Store + 'static,
    B: blockstore::Blockstore + MaybeSendSync + 'static,
{
    db.set_merge_governance(governance);
    install_local_write_judge(db, blockstore, max_merge_depth);
}
