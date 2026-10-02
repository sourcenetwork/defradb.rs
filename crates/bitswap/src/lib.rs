//! Bitswap block exchange, wire-compatible with Go DefraDB: protobuf messages, protocol ids, the
//! length-prefixed codec, the connection handler, the network behaviour, the block-serving server and the fetching client.

pub mod behaviour;
pub mod block;
pub mod client;
pub mod codec;
pub mod error;
pub mod handler;
pub mod handler_error;
pub mod message;
pub mod network;
pub mod pb;
mod peer_state;
pub mod prefix;
pub mod protocol;
pub mod server;
pub mod store;

pub use behaviour::{Bitswap, BitswapEvent, Config};
pub use block::Block;
pub use client::FetchId;
pub use codec::BitswapCodec;
pub use error::Error;
pub use handler_error::BitswapHandlerError;
pub use message::{BitswapMessage, BlockPresence, BlockPresenceType, Entry, Priority, WantType};
pub use prefix::Prefix;
pub use protocol::{ProtocolConfig, ProtocolId};
pub use server::{PeerBlockRequestFilter, ServerConfig};
pub use store::Store;
