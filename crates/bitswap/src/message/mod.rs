//! The in-memory bitswap message with the wantlist merge rules of the reference implementation.

mod decode;
mod encode;
mod entry;
mod presence;

use std::fmt::{self, Debug};

use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use rapidhash::RapidHashMap;
use tracing::warn;

use crate::block::Block;

pub use entry::{Entry, Priority, WantType};
pub use presence::{BlockPresence, BlockPresenceType};

/// A bitswap message.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct BitswapMessage {
    full: bool,
    wantlist: RapidHashMap<Cid, Entry>,
    blocks: RapidHashMap<Cid, Block>,
    block_presences: RapidHashMap<Cid, BlockPresenceType>,
    pending_bytes: i32,
}

struct Fmt<F>(F)
where
    F: Fn(&mut fmt::Formatter) -> fmt::Result;

impl<F> Debug for Fmt<F>
where
    F: Fn(&mut fmt::Formatter) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (self.0)(f)
    }
}

fn debug_map<V: Debug>(map: &RapidHashMap<Cid, V>) -> impl Debug + '_ {
    Fmt(move |f| {
        let mut out = f.debug_map();
        for (cid, v) in map {
            out.entry(&cid.to_string(), v);
        }
        out.finish()
    })
}

impl Debug for BitswapMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitswapMessge")
            .field("full", &self.full)
            .field("wantlist", &debug_map(&self.wantlist))
            .field("blocks", &debug_map(&self.blocks))
            .field("block_presences", &debug_map(&self.block_presences))
            .field("pending_bytes", &self.pending_bytes)
            .finish()
    }
}

impl BitswapMessage {
    /// An empty message; `full` marks the wantlist as complete.
    pub fn new(full: bool) -> Self {
        BitswapMessage {
            full,
            ..Default::default()
        }
    }

    /// Clears all contents of this message for it to be reused.
    pub fn clear(&mut self, full: bool) {
        self.full = full;
        self.wantlist.clear();
        self.blocks.clear();
        self.block_presences.clear();
        self.pending_bytes = 0;
    }

    /// Whether the wantlist is the full wantlist.
    pub fn full(&self) -> bool {
        self.full
    }

    /// Removes all blocks whose data does not hash to their cid.
    pub fn verify_blocks(&mut self) {
        self.blocks
            .retain(|_, block| match verify_hash(&block.cid, &block.data) {
                Some(true) => true,
                Some(false) => {
                    warn!("invalid block received");
                    false
                }
                None => {
                    warn!("unknown hash function {}", block.cid.hash().code());
                    false
                }
            });
    }

    /// True when there are no blocks, wants or presences.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.wantlist.is_empty() && self.block_presences.is_empty()
    }

    /// The wantlist entries.
    pub fn wantlist(&self) -> impl Iterator<Item = &Entry> {
        self.wantlist.values()
    }

    /// Number of blocks.
    pub fn blocks_len(&self) -> usize {
        self.blocks.len()
    }

    /// The blocks.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.values()
    }

    /// The block presences.
    pub fn block_presences(&self) -> impl Iterator<Item = BlockPresence> + '_ {
        self.block_presences.iter().map(|(cid, typ)| BlockPresence {
            cid: *cid,
            typ: *typ,
        })
    }

    /// Cids announced as HAVE.
    pub fn haves(&self) -> impl Iterator<Item = &Cid> {
        self.presences_of(BlockPresenceType::Have)
    }

    /// Cids announced as DONT_HAVE.
    pub fn dont_haves(&self) -> impl Iterator<Item = &Cid> {
        self.presences_of(BlockPresenceType::DontHave)
    }

    fn presences_of(&self, typ: BlockPresenceType) -> impl Iterator<Item = &Cid> {
        self.block_presences
            .iter()
            .filter_map(move |(cid, t)| (*t == typ).then_some(cid))
    }

    /// Bytes the sender still has queued.
    pub fn pending_bytes(&self) -> i32 {
        self.pending_bytes
    }

    /// Sets the pending bytes.
    pub fn set_pending_bytes(&mut self, bytes: i32) {
        self.pending_bytes = bytes;
    }

    /// Drops the wantlist entry for the cid.
    pub fn remove(&mut self, cid: &Cid) {
        self.wantlist.remove(cid);
    }

    /// Adds a cancel entry; returns the encoded size added.
    pub fn cancel(&mut self, cid: Cid) -> usize {
        self.add_full_entry(cid, 0, true, WantType::Block, false)
    }

    /// Adds a want entry; returns the encoded size added.
    pub fn add_entry(
        &mut self,
        cid: Cid,
        priority: Priority,
        want_type: WantType,
        send_dont_have: bool,
    ) -> usize {
        self.add_full_entry(cid, priority, false, want_type, send_dont_have)
    }

    fn add_full_entry(
        &mut self,
        cid: Cid,
        priority: Priority,
        cancel: bool,
        want_type: WantType,
        send_dont_have: bool,
    ) -> usize {
        if let Some(entry) = self.wantlist.get_mut(&cid) {
            if entry.want_type == want_type {
                entry.priority = priority;
            }
            if cancel {
                entry.cancel = true;
            }
            if send_dont_have {
                entry.send_dont_have = true;
            }
            if want_type == WantType::Block && entry.want_type == WantType::Have {
                entry.want_type = WantType::Block;
            }
            return 0;
        }

        let entry = Entry {
            cid,
            priority,
            want_type,
            send_dont_have,
            cancel,
        };
        let size = entry.encoded_len();
        self.wantlist.insert(cid, entry);
        size
    }

    /// Adds a block, replacing any presence for its cid.
    pub fn add_block(&mut self, block: Block) {
        self.block_presences.remove(block.cid());
        self.blocks.insert(*block.cid(), block);
    }

    /// Adds a presence unless the block itself is present.
    pub fn add_block_presence(&mut self, cid: Cid, typ: BlockPresenceType) {
        if self.blocks.contains_key(&cid) {
            return;
        }
        self.block_presences.insert(cid, typ);
    }

    /// Adds a HAVE.
    pub fn add_have(&mut self, cid: Cid) {
        self.add_block_presence(cid, BlockPresenceType::Have);
    }

    /// Adds a DONT_HAVE.
    pub fn add_dont_have(&mut self, cid: Cid) {
        self.add_block_presence(cid, BlockPresenceType::DontHave);
    }

    /// Approximate encoded size: block data plus presences plus entries.
    pub fn encoded_len(&self) -> usize {
        let blocks: usize = self.blocks.values().map(|b| b.data.len()).sum();
        let presences: usize = self.block_presences().map(|bp| bp.encoded_len()).sum();
        let wants: usize = self.wantlist.values().map(|e| e.encoded_len()).sum();
        blocks + presences + wants
    }
}

fn verify_hash(cid: &Cid, bytes: &[u8]) -> Option<bool> {
    Code::try_from(cid.hash().code())
        .ok()
        .map(|code| &code.digest(bytes) == cid.hash())
}
