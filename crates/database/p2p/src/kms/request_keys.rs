//! How many keys one KMS request may ask for when raw publishes are bounded.
//!
//! A request carries one CID per key, and its reply one CID and one
//! ECIES-wrapped key block per key, inside the response envelope. The reply
//! is the larger, so it sets the limit. Both sizes are measured against real
//! encodings in `tests/kms/request_keys.rs`.

/// What a reply's envelope holds besides its keys: 152 bytes measured with
/// no key (the envelope's map, its request ID, empty error and sender, the
/// reply's map and the responder's ephemeral key), plus the bytes its list
/// and data headers grow by as keys are added.
pub(super) const REPLY_FIXED: usize = 160;

/// What each key adds to a reply, measured: its 36-byte CID and its 99-byte
/// wrapped key block, each with a two-byte header.
pub(super) const REPLY_PER_KEY: usize = 139;

/// The most keys whose reply fits `limit` bytes.
pub(super) fn keys_per_request(limit: usize) -> usize {
    limit.saturating_sub(REPLY_FIXED) / REPLY_PER_KEY
}

#[cfg(test)]
#[path = "../../tests/kms/request_keys.rs"]
mod tests;
