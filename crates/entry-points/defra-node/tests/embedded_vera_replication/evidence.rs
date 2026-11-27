//! What a receiver holds of a change it has not applied: the change's
//! composite block, received over the network and kept in its blockstore
//! with the marker that says it is not merged yet.

use std::collections::BTreeSet;
use std::sync::Arc;

use blockstore::Blockstore as _;
use identity::Did;

use super::peer::Peer;

impl Peer {
    /// How many of the blocks `cids` this peer holds without having merged
    /// them.
    pub async fn held_unmerged(&self, cids: &[String]) -> usize {
        let database = self.node.database();
        let blocks = blockstore::DefraBlockstore::new(Arc::clone(database.store()), true)
            .placed_on(database.pool());
        let unmerged: BTreeSet<String> = blocks
            .get_unmerged()
            .await
            .expect("unmerged blocks")
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut held = 0;
        for cid in cids.iter().filter(|cid| unmerged.contains(*cid)) {
            let parsed: cid::Cid = cid.parse().expect("CID");
            held += usize::from(blocks.has(&parsed).await.expect("block lookup"));
        }
        held
    }

    /// Whether this peer holds the change `cid` to `doc` and has not
    /// applied it: the block is kept unmerged, and the document's history
    /// as `reader` sees it does not list it.
    pub async fn holds_unapplied(&self, reader: &Did, doc: &str, cid: &str) -> bool {
        self.held_unmerged(&[cid.to_owned()]).await == 1
            && !self
                .commits(reader, doc)
                .await
                .iter()
                .any(|held| held == cid)
    }

    /// The composite commits of `doc` on this peer that `before` does not
    /// list: the changes made since `before` was read.
    pub async fn commits_since(&self, reader: &Did, doc: &str, before: &[String]) -> Vec<String> {
        self.commits(reader, doc)
            .await
            .into_iter()
            .filter(|cid| !before.contains(cid))
            .collect()
    }
}
