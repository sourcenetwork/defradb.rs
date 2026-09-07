//! Grants that ride along with the documents this client authored.

use std::str::FromStr;

use cid::Cid;
use defra_core::block::{Block, Signature};
use defra_core::browser_sync::{BrowserSyncDocument, BrowserSyncRelationship};

/// Attach `grants` to a document this client signed.
///
/// The node applies a pushed grant as the caller and refuses one on a document
/// the caller does not own, which would fail the whole push — so a relayed
/// document goes out as it arrived.
pub(super) fn attach_grants(
    document: &mut BrowserSyncDocument,
    grants: &[BrowserSyncRelationship],
    signer_identity: &[u8],
) {
    if !grants.is_empty() && authored_by(document, signer_identity) {
        document.relationships = grants.to_vec();
    }
}

/// Whether the document's genesis block is signed by this identity, which is
/// what the node registers ownership from.
fn authored_by(document: &BrowserSyncDocument, signer_identity: &[u8]) -> bool {
    let Some(genesis) = document.blocks.iter().find_map(|block| {
        let cid = Cid::from_str(&block.cid).ok()?;
        (db::block::builder::derive_doc_id(&cid) == document.doc_id).then_some(block)
    }) else {
        return false;
    };
    let Some(signature_cid) = hex::decode(&genesis.data)
        .ok()
        .and_then(|bytes| Block::from_dag_cbor(&bytes).ok())
        .and_then(|block| block.signature)
    else {
        return false;
    };
    let signature_cid = signature_cid.to_string();

    document
        .blocks
        .iter()
        .find(|block| block.cid == signature_cid)
        .and_then(|block| hex::decode(&block.data).ok())
        .and_then(|bytes| Signature::from_dag_cbor(&bytes).ok())
        .is_some_and(|signature| signature.header.identity == signer_identity)
}
