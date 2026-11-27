//! Decoding a protobuf message into a [`BitswapMessage`].

use bytes::Bytes;
use cid::Cid;
use prost::Message;

use super::BitswapMessage;
use crate::block::Block;
use crate::error::Error;
use crate::pb;
use crate::prefix::Prefix;

impl TryFrom<pb::Message> for BitswapMessage {
    type Error = Error;

    fn try_from(pbm: pb::Message) -> Result<Self, Self::Error> {
        let full = pbm.wantlist.as_ref().is_some_and(|w| w.full);
        let mut message = BitswapMessage::new(full);

        if let Some(wantlist) = pbm.wantlist {
            for entry in wantlist.entries {
                let cid = Cid::try_from(entry.block)?;
                message.add_full_entry(
                    cid,
                    entry.priority,
                    entry.cancel,
                    entry.want_type.try_into()?,
                    entry.send_dont_have,
                );
            }
        }

        for data in pbm.blocks {
            message.add_block(Block::from_v0_data(data)?);
        }

        for block in pbm.payload {
            let cid = Prefix::new(&block.prefix)?.to_cid(&block.data)?;
            message.add_block(Block::new(block.data, cid));
        }

        for block_presence in pbm.block_presences {
            let cid = Cid::try_from(block_presence.cid)?;
            message.add_block_presence(cid, block_presence.r#type.try_into()?);
        }

        message.pending_bytes = pbm.pending_bytes;
        Ok(message)
    }
}

impl TryFrom<Bytes> for BitswapMessage {
    type Error = Error;

    fn try_from(value: Bytes) -> Result<Self, Self::Error> {
        pb::Message::decode(value)?.try_into()
    }
}
