#![allow(dead_code)]

use bitswap::Block;
use bytes::Bytes;
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};

const RAW: u64 = 0x55;

pub fn cid_v1(seed: &[u8]) -> Cid {
    Cid::new_v1(RAW, Code::Sha2_256.digest(seed))
}

pub fn block_v1(data: &[u8]) -> Block {
    Block::new(Bytes::copy_from_slice(data), cid_v1(data))
}

pub fn block_v0(data: &[u8]) -> Block {
    Block::from_v0_data(Bytes::copy_from_slice(data)).unwrap()
}

pub mod fixtures;
pub mod mem_store;
pub mod probe;
