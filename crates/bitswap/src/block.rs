//! A block of bytes paired with its cid.

use std::fmt::Debug;

use bytes::Bytes;
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

/// A wrapper around bytes with their `Cid`.
#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct Block {
    /// The block cid.
    pub cid: Cid,
    /// The block data.
    pub data: Bytes,
}

impl Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Block")
            .field("cid", &self.cid.to_string())
            .field("data", &format!("[{} bytes]", self.data.len()))
            .finish()
    }
}

impl Block {
    /// Pairs data with a cid.
    pub fn new(data: Bytes, cid: Cid) -> Self {
        Self { cid, data }
    }

    /// Builds a CIDv0 block from the sha2-256 of the data.
    pub fn from_v0_data(data: Bytes) -> cid::Result<Self> {
        let digest = Code::Sha2_256.digest(&data);
        let cid = Cid::new_v0(digest)?;
        Ok(Self { cid, data })
    }

    /// The block cid.
    pub fn cid(&self) -> &Cid {
        &self.cid
    }

    /// The block data.
    pub fn data(&self) -> &Bytes {
        &self.data
    }
}
