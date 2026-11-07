//! Hostile blocks pushed to a real node by an in-process iroh peer.
//!
//! Each refusal test first proves the pipe works: the same peer pushes an
//! honest payload and waits for it to read back, so an absent hostile document
//! means refused rather than never delivered.

mod blocks;
mod cid_mismatch;
mod doc_id_mismatch;
mod forged_signature;
mod peer;
mod permitted_update;
mod protected;
mod protected_ancestor;
mod protected_delete;
mod protected_update;
mod receiver;
mod signature_hop;
