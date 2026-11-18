//! What a rooted transport's (vinc, iroh) fetch request names.
//!
//! The provider walks a document's composite spine only when every block
//! asked for is a composite; then the receiver's haves leave out the history
//! it holds. A field block asked for takes the depth-cut closure instead, which
//! sends held field history back, and a doubling cut walks the field chains
//! again each round. So a document's fetch names its root, with no cut below
//! the response cap, when the provider can leave out everything the receiver
//! holds: the receiver names haves or present blocks, or holds nothing below
//! the root (gossip handed it only the new head). One request then carries
//! exactly the missing region, or the whole DAG. A receiver that holds blocks
//! below the root but names neither (pushed blocks not merged yet), a branchable
//! collection's blocks (an empty document ID) and a root that is not a
//! composite keep the missing CIDs and the doubling cut, since naming the
//! root would send those held blocks back.

use blockstore::Blockstore;
use cid::Cid;
use defra_core::{Block, CrdtDelta};

use super::closure_depth::ClosureDepth;
use super::dag_context::DagFetchContext;
use crate::sync::car::extract_links;

pub(crate) async fn rooted_request<B>(
    blockstore: &B,
    root_cid: &Cid,
    missing: &[Cid],
    context: &DagFetchContext,
    depth: ClosureDepth,
) -> (Vec<Cid>, ClosureDepth)
where
    B: Blockstore,
{
    let frontier = (missing.to_vec(), depth);
    if context.doc_id.is_empty() {
        return frontier;
    }
    let data = match blockstore.get(root_cid).await {
        Ok(Some(data)) => data,
        _ => return (vec![*root_cid], ClosureDepth::whole()),
    };
    let composite = Block::from_dag_cbor(&data)
        .is_ok_and(|block| matches!(block.delta, CrdtDelta::Composite(_)));
    if !composite {
        return frontier;
    }
    if context.haves().is_empty() && context.present().is_empty() {
        for link in extract_links(&data) {
            if matches!(blockstore.has(&link).await, Ok(true)) {
                return frontier;
            }
        }
    }
    (vec![*root_cid], ClosureDepth::whole())
}
