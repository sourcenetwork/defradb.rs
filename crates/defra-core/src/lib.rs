//! Core types and traits for DefraDB
//!
//! This crate defines the fundamental types, traits, and interfaces
//! used throughout the DefraDB ecosystem.

pub mod action;
#[cfg(any(test, feature = "ambient"))]
pub mod ambient;
pub mod batch_signing;
pub mod block;
pub mod block_delta;
mod block_encode;
pub mod block_signature;
pub mod cbor;
pub mod cid_encoding;
pub mod collection;
/// The acting identity tests set the old way (see [`ambient`]).
#[cfg(any(test, feature = "ambient"))]
pub mod current_identity;
/// The DAC bypass flag tests set the old way (see [`ambient`]).
#[cfg(any(test, feature = "ambient"))]
pub mod dac_bypass {
    pub use crate::ambient::{get_dac_bypass, set_dac_bypass};
}
pub mod doc_id;
pub mod document;
pub mod encryption;
pub mod error;
pub mod ipld;
pub mod json_crdt;
pub mod lens_block;
pub mod merge;
pub mod request_context;
pub mod signing;
pub mod store;
pub mod text_op;
pub mod thread_bounds;
pub mod transaction;
pub mod types;
pub mod vector;
#[cfg(any(test, all(target_arch = "wasm32", feature = "ambient")))]
pub mod wasm_task_local;

pub use action::{Action, ActionExecution, ActionStatus};
pub use block::{
    Block, CollectionDefinitionDeltaPayload, CollectionDeltaPayload, CollectionSetDeltaPayload,
    CompositeDeltaPayload, CounterDeltaPayload, CrdtDelta, CustomDeltaPayload, DAGLink,
    DocumentStatus, Encryption, FieldDefinitionDeltaPayload, ImmutableDeltaPayload,
    JsonDeltaPayload, LwwDeltaPayload, Signature, SignatureHeader, SignatureType, TextDeltaPayload,
    DAG_CBOR_CODEC, IMMUTABLE_LINK_NAME, SHA2_256_CODE,
};
pub use collection::collection_short_id;
pub use encryption::EncryptionKey;
pub use error::{Error, Result};
pub use ipld::{collect_block_links, extract_links, walk_ipld, IpldVisitor};
pub use lens_block::{
    build_lens_ipld_blocks, is_lens_block, CidBlock, LensConfigBlock, LensKeyValue,
    LensModuleBlock, LensWasmBlock,
};
pub use request_context::RequestContext;
pub use signing::SigningKeyType;
pub use text_op::{TextEdits, TextOp};

/// Version information for DefraDB
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
