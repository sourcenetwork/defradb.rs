//! The missing region of a DAG fetch: the part of the wanted closure that
//! lies outside the closures of the merged blocks the receiver names.
//!
//! A document's composite blocks form its spine: each links its previous
//! composites through `heads`, at a priority one above the highest of them,
//! and carries the blocks of its own update (field blocks, its signature) as
//! attachments. A field block's own `heads` reach older field blocks, which
//! are attachments of older composites. So the receiver lacks exactly the
//! composites below the wanted ones that no held composite reaches, plus
//! their attachments.
//!
//! The walk pops spine blocks highest priority first. A held composite
//! passes "held" to the composites it links; a wanted one is served with its
//! attachments and passes "wanted" on, unless the block is already held.
//! Every composite a block links has a lower priority, so a block's colour
//! is final before it is popped. The walk ends when nothing wanted is left.

use std::collections::BinaryHeap;

use blockstore::Blockstore;
use bytes::Bytes;
use cid::Cid;
use defra_core::{Block, CrdtDelta};
use rapidhash::{RapidHashMap, RapidHashSet};

use super::{
    collect_dag_blocks_within, extract_links, CarCollectOutcome, CAR_MAX_BLOCKS, CAR_MAX_BYTES,
};
use crate::error::{Error, Result};

/// The most have CIDs a responder reads from one request.
pub const MAX_HAVE_CIDS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Colour {
    Held,
    Wanted,
}

/// What the walk needs from a block's bytes.
struct Shape {
    priority: u64,
    spine: bool,
    heads: Vec<Cid>,
    attachments: Vec<Cid>,
}

impl Shape {
    fn of(data: &[u8]) -> Self {
        let links = extract_links(data);
        let Ok(block) = Block::from_dag_cbor(data) else {
            return Self {
                priority: 0,
                spine: false,
                heads: Vec::new(),
                attachments: links,
            };
        };
        let heads = block.heads.unwrap_or_default();
        let attachments = links
            .into_iter()
            .filter(|cid| !heads.contains(cid))
            .collect();
        Self {
            priority: block.delta.priority(),
            spine: matches!(block.delta, CrdtDelta::Composite(_)),
            heads,
            attachments,
        }
    }
}

struct Entry {
    colour: Colour,
    /// A wanted CID a held block links: served, but its closure is held.
    held_below: bool,
    popped: bool,
    data: Bytes,
    shape: Shape,
}

#[derive(Default)]
struct Walk {
    entries: RapidHashMap<Cid, Entry>,
    queue: BinaryHeap<(u64, Cid)>,
    pending_wanted: usize,
    held_reads: usize,
    total_bytes: usize,
    served: RapidHashSet<Cid>,
    wants: RapidHashSet<Cid>,
    outcome: CarCollectOutcome,
}

impl Walk {
    fn insert(&mut self, cid: Cid, data: Bytes, colour: Colour) {
        let shape = Shape::of(&data);
        self.queue.push((shape.priority, cid));
        if colour == Colour::Wanted {
            self.pending_wanted += 1;
        }
        self.entries.insert(
            cid,
            Entry {
                colour,
                held_below: false,
                popped: false,
                data,
                shape,
            },
        );
    }

    fn room(&mut self) -> bool {
        if self.served.len() + self.pending_wanted >= CAR_MAX_BLOCKS {
            self.outcome.truncated_by_blocks = true;
            return false;
        }
        true
    }

    async fn read_wanted<B>(&mut self, blockstore: &B, cid: &Cid) -> Result<Option<Bytes>>
    where
        B: Blockstore,
    {
        let remaining = CAR_MAX_BYTES - self.total_bytes;
        let data = self.outcome.read_block(blockstore, cid, remaining).await?;
        if let Some(data) = &data {
            self.total_bytes += data.len();
        }
        Ok(data)
    }

    async fn mark_held<B>(&mut self, blockstore: &B, cid: Cid) -> Result<()>
    where
        B: Blockstore,
    {
        if let Some(entry) = self.entries.get_mut(&cid) {
            if entry.colour == Colour::Wanted && !entry.popped {
                if self.wants.contains(&cid) {
                    entry.held_below = true;
                } else {
                    entry.colour = Colour::Held;
                    self.pending_wanted -= 1;
                    self.total_bytes -= entry.data.len();
                }
            }
            return Ok(());
        }
        if self.held_reads >= CAR_MAX_BLOCKS {
            return Ok(());
        }
        self.held_reads += 1;
        let data = blockstore
            .get(&cid)
            .await
            .map_err(|e| Error::BlockstoreError(format!("failed to get block {}: {}", cid, e)))?;
        if let Some(data) = data {
            self.insert(cid, data, Colour::Held);
        }
        Ok(())
    }

    async fn mark_wanted<B>(&mut self, blockstore: &B, cid: Cid) -> Result<()>
    where
        B: Blockstore,
    {
        if self.entries.contains_key(&cid) || self.served.contains(&cid) || !self.room() {
            return Ok(());
        }
        if let Some(data) = self.read_wanted(blockstore, &cid).await? {
            self.insert(cid, data, Colour::Wanted);
        }
        Ok(())
    }

    fn serve(&mut self, cid: Cid, data: Bytes) {
        if self.served.insert(cid) {
            self.outcome.blocks.push((cid, data));
        }
    }

    /// Serve a wanted block's attachments and theirs, never their `heads`.
    async fn serve_attachments<B>(&mut self, blockstore: &B, roots: Vec<Cid>) -> Result<()>
    where
        B: Blockstore,
    {
        let mut stack = roots;
        while let Some(cid) = stack.pop() {
            if self.entries.contains_key(&cid) || self.served.contains(&cid) || !self.room() {
                continue;
            }
            let Some(data) = self.read_wanted(blockstore, &cid).await? else {
                continue;
            };
            stack.extend(Shape::of(&data).attachments);
            self.serve(cid, data);
        }
        Ok(())
    }
}

/// Collect the part of the closure below `wants` that is outside the closures
/// of `haves`, under the same caps as [`collect_dag_blocks_within`].
///
/// A wanted CID is always served, even when a have reaches it: the receiver
/// asked for it because it lacks it. With no have this responder holds, the
/// walk falls back to [`collect_dag_blocks_within`] with `max_depth`.
pub async fn collect_missing_region<B>(
    blockstore: &B,
    wants: &[Cid],
    haves: &[Cid],
    max_depth: Option<u32>,
) -> Result<CarCollectOutcome>
where
    B: Blockstore,
{
    let mut walk = Walk::default();
    for have in haves.iter().take(MAX_HAVE_CIDS) {
        walk.mark_held(blockstore, *have).await?;
    }
    if walk.entries.is_empty() {
        return collect_dag_blocks_within(blockstore, wants, max_depth).await;
    }
    for want in wants {
        if !walk.wants.insert(*want) || !walk.room() {
            continue;
        }
        if let Some(data) = walk.read_wanted(blockstore, want).await? {
            let named_held = walk.entries.remove(want).is_some();
            walk.insert(*want, data, Colour::Wanted);
            if let Some(entry) = walk.entries.get_mut(want) {
                entry.held_below = named_held;
            }
        }
    }

    while walk.pending_wanted > 0 {
        let Some((_, cid)) = walk.queue.pop() else {
            break;
        };
        let Some(entry) = walk.entries.get_mut(&cid) else {
            continue;
        };
        if entry.popped {
            continue;
        }
        entry.popped = true;
        let colour = entry.colour;
        let held_below = entry.held_below;
        let spine = entry.shape.spine;
        let heads = entry.shape.heads.clone();
        match colour {
            Colour::Held => {
                if spine {
                    for head in heads {
                        walk.mark_held(blockstore, head).await?;
                    }
                }
            }
            Colour::Wanted => {
                walk.pending_wanted -= 1;
                let data = entry.data.clone();
                let attachments = entry.shape.attachments.clone();
                walk.serve(cid, data);
                walk.serve_attachments(blockstore, attachments).await?;
                if spine {
                    for head in heads {
                        if held_below {
                            walk.mark_held(blockstore, head).await?;
                        } else {
                            walk.mark_wanted(blockstore, head).await?;
                        }
                    }
                }
            }
        }
    }
    Ok(walk.outcome)
}

#[cfg(test)]
#[path = "../../../tests/unit/car_region.rs"]
mod tests;
