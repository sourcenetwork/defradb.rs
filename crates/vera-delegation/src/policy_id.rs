//! A policy identifier as vera.rs takes it.

use alloy_primitives::FixedBytes;

/// The 32 bytes the hexadecimal policy identifier `hex` names, with or
/// without a `0x` prefix. vera.rs's client parses it the same way, with the
/// same error text.
pub fn parse_policy_id(hex: &str) -> Result<FixedBytes<32>, PolicyIdError> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).map_err(PolicyIdError::Hex)?;
    if bytes.len() != 32 {
        return Err(PolicyIdError::Length(bytes.len()));
    }
    Ok(FixedBytes::from_slice(&bytes))
}

/// Why a policy identifier does not name a policy.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyIdError {
    #[error("ABI decode error: invalid policy ID: {0}")]
    Hex(hex::FromHexError),

    #[error("ABI decode error: policy ID must be 32 bytes, got {0}")]
    Length(usize),
}
