//! Wantlist entries.

use std::fmt::{self, Debug};

use cid::Cid;
use prost::Message;

use crate::error::Error;
use crate::pb::message::wantlist as pbw;

/// Priority of a wanted block.
pub type Priority = i32;

/// What the requester wants for a cid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum WantType {
    /// The block itself.
    Block = 0,
    /// Only a HAVE.
    Have = 1,
}

impl TryFrom<i32> for WantType {
    type Error = Error;

    fn try_from(v: i32) -> Result<Self, Error> {
        match v {
            0 => Ok(WantType::Block),
            1 => Ok(WantType::Have),
            v => Err(Error::InvalidWantType(v)),
        }
    }
}

impl From<WantType> for pbw::WantType {
    fn from(want: WantType) -> Self {
        match want {
            WantType::Block => pbw::WantType::Block,
            WantType::Have => pbw::WantType::Have,
        }
    }
}

/// A wantlist entry with cancel, DONT_HAVE and HAVE-only flags.
#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    /// The wanted cid.
    pub cid: Cid,
    /// Priority of the want.
    pub priority: Priority,
    /// Block or HAVE.
    pub want_type: WantType,
    /// Whether this revokes an entry.
    pub cancel: bool,
    /// Whether the requester wants a DONT_HAVE.
    pub send_dont_have: bool,
}

impl Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("cid", &self.cid.to_string())
            .field("priority", &self.priority)
            .field("want_type", &self.want_type)
            .field("cancel", &self.cancel)
            .field("send_dont_have", &self.send_dont_have)
            .finish()
    }
}

impl Entry {
    /// Returns the encoded length of this entry.
    pub fn encoded_len(&self) -> usize {
        pbw::Entry::from(self).encoded_len()
    }
}

impl From<&Entry> for pbw::Entry {
    fn from(e: &Entry) -> Self {
        pbw::Entry {
            block: e.cid.to_bytes(),
            priority: e.priority,
            want_type: pbw::WantType::from(e.want_type) as i32,
            cancel: e.cancel,
            send_dont_have: e.send_dont_have,
        }
    }
}
