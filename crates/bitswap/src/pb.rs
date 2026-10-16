//! Hand-written prost mirror of `bitswap_pb.proto`; fields are in tag order to match prost-build output.

use bytes::Bytes;

/// The top-level bitswap protobuf message.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    /// Wantlist, always present when encoding.
    #[prost(message, optional, tag = "1")]
    pub wantlist: Option<message::Wantlist>,
    /// Raw block data (bitswap 1.0.0).
    #[prost(bytes = "bytes", repeated, tag = "2")]
    pub blocks: Vec<Bytes>,
    /// Prefixed blocks (bitswap 1.1.0).
    #[prost(message, repeated, tag = "3")]
    pub payload: Vec<message::Block>,
    /// HAVE / DONT_HAVE announcements.
    #[prost(message, repeated, tag = "4")]
    pub block_presences: Vec<message::BlockPresence>,
    /// Bytes the sender still has queued.
    #[prost(int32, tag = "5")]
    pub pending_bytes: i32,
}

/// Nested message types.
pub mod message {
    use bytes::Bytes;

    /// A list of wanted blocks.
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Wantlist {
        /// The entries.
        #[prost(message, repeated, tag = "1")]
        pub entries: Vec<wantlist::Entry>,
        /// Whether this is the full wantlist.
        #[prost(bool, tag = "2")]
        pub full: bool,
    }

    /// Wantlist nested types.
    pub mod wantlist {
        /// Kind of want.
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum WantType {
            /// Want the block.
            Block = 0,
            /// Want only a HAVE.
            Have = 1,
        }

        /// One wantlist entry.
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Entry {
            /// Block cid bytes.
            #[prost(bytes = "vec", tag = "1")]
            pub block: Vec<u8>,
            /// Normalized priority.
            #[prost(int32, tag = "2")]
            pub priority: i32,
            /// Whether this revokes an entry.
            #[prost(bool, tag = "3")]
            pub cancel: bool,
            /// Want kind.
            #[prost(enumeration = "WantType", tag = "4")]
            pub want_type: i32,
            /// Whether the requester wants a DONT_HAVE.
            #[prost(bool, tag = "5")]
            pub send_dont_have: bool,
        }
    }

    /// A block with its cid prefix.
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Block {
        /// Cid version, codec and multihash type and length.
        #[prost(bytes = "vec", tag = "1")]
        pub prefix: Vec<u8>,
        /// Block data.
        #[prost(bytes = "bytes", tag = "2")]
        pub data: Bytes,
    }

    /// Kind of presence.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum BlockPresenceType {
        /// The sender has the block.
        Have = 0,
        /// The sender lacks the block.
        DontHave = 1,
    }

    /// A HAVE / DONT_HAVE for one cid.
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct BlockPresence {
        /// Cid bytes.
        #[prost(bytes = "vec", tag = "1")]
        pub cid: Vec<u8>,
        /// Presence kind.
        #[prost(enumeration = "BlockPresenceType", tag = "2")]
        pub r#type: i32,
    }
}
