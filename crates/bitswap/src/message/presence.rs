//! HAVE / DONT_HAVE announcements.

use cid::Cid;
use prost::Message;

use crate::error::Error;
use crate::pb::message as pbm;

/// Represents a HAVE / DONT_HAVE for a given Cid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPresence {
    /// The cid.
    pub cid: Cid,
    /// Have or don't have.
    pub typ: BlockPresenceType,
}

impl BlockPresence {
    /// Returns the encoded length of this presence.
    pub fn encoded_len(&self) -> usize {
        pbm::BlockPresence::from(self).encoded_len()
    }

    /// Encoded length of a presence for the cid.
    pub fn encoded_len_for_cid(cid: Cid) -> usize {
        pbm::BlockPresence {
            cid: cid.to_bytes(),
            r#type: pbm::BlockPresenceType::Have as i32,
        }
        .encoded_len()
    }
}

impl From<&BlockPresence> for pbm::BlockPresence {
    fn from(bp: &BlockPresence) -> Self {
        pbm::BlockPresence {
            cid: bp.cid.to_bytes(),
            r#type: pbm::BlockPresenceType::from(bp.typ) as i32,
        }
    }
}

/// Whether the sender has the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum BlockPresenceType {
    /// The sender has the block.
    Have = 0,
    /// The sender lacks the block.
    DontHave = 1,
}

impl TryFrom<i32> for BlockPresenceType {
    type Error = Error;

    fn try_from(v: i32) -> Result<Self, Error> {
        match v {
            0 => Ok(BlockPresenceType::Have),
            1 => Ok(BlockPresenceType::DontHave),
            v => Err(Error::InvalidBlockPresenceType(v)),
        }
    }
}

impl From<BlockPresenceType> for pbm::BlockPresenceType {
    fn from(ty: BlockPresenceType) -> Self {
        match ty {
            BlockPresenceType::Have => pbm::BlockPresenceType::Have,
            BlockPresenceType::DontHave => pbm::BlockPresenceType::DontHave,
        }
    }
}
