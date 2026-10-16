//! Encoding a message as the v0 (1.0.0) or v1 (1.1.0 and later) protobuf.

use super::BitswapMessage;
use crate::error::Error;
use crate::pb;
use crate::prefix::Prefix;

impl BitswapMessage {
    fn proto_with_wantlist(&self) -> pb::Message {
        pb::Message {
            wantlist: Some(pb::message::Wantlist {
                entries: self.wantlist.values().map(Into::into).collect(),
                full: self.full,
            }),
            ..Default::default()
        }
    }

    /// Encodes for bitswap 1.0.0 and legacy: raw block data, no presences.
    pub fn encode_as_proto_v0(&self) -> pb::Message {
        let mut message = self.proto_with_wantlist();
        message.blocks = self.blocks.values().map(|b| b.data().clone()).collect();
        message
    }

    /// Encodes for bitswap 1.1.0 and 1.2.0: prefixed payload, presences, pending bytes.
    pub fn encode_as_proto_v1(&self) -> Result<pb::Message, Error> {
        let mut message = self.proto_with_wantlist();
        for block in self.blocks.values() {
            message.payload.push(pb::message::Block {
                prefix: Prefix::try_from(block.cid())?.to_bytes(),
                data: block.data().clone(),
            });
        }
        message.block_presences = self
            .block_presences
            .iter()
            .map(|(cid, typ)| pb::message::BlockPresence {
                cid: cid.to_bytes(),
                r#type: pb::message::BlockPresenceType::from(*typ) as i32,
            })
            .collect();
        message.pending_bytes = self.pending_bytes;
        Ok(message)
    }
}
