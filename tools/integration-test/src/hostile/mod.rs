//! A peer that is not a defra node: it builds commits from the production
//! block types, honest or tampered, and pushes them to a real node over iroh.

pub mod blocks;
pub mod peer;
