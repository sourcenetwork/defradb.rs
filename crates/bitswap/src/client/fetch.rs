//! Bookkeeping for one `fetch` call: where its blocks are delivered and which cids are still pending.

use cid::Cid;
use rapidhash::RapidHashSet;
use tokio::sync::mpsc;

use crate::block::Block;

/// Identifies one `fetch` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FetchId(pub(crate) u64);

#[derive(Debug)]
pub(crate) struct Fetch {
    pub(crate) sender: mpsc::Sender<Block>,
    pub(crate) pending: RapidHashSet<Cid>,
}
