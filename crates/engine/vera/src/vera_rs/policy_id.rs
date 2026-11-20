//! The ID vera.rs gives a policy, known before the policy exists.

use vera_client::PolicyMarshalingType;

use super::did_spelling;
use crate::provider::ProviderError;

/// The ID vera.rs gives the YAML `policy` created by `creator` under
/// `nonce`: SHA-256 over the policy's hash, the nonce and the creator's DID,
/// as `vera_client::predict_policy_id` derives it. `creator` is spelled as
/// vera.rs records it first, so either spelling of a key gives one ID.
///
/// # Errors
/// The definition does not compile.
pub fn predict_policy_id(
    policy: &str,
    nonce: acp::PolicyNonce,
    creator: &str,
) -> Result<String, ProviderError> {
    vera_client::predict_policy_id(
        policy,
        &PolicyMarshalingType::ShortYaml,
        &vera_client::PolicyNonce(nonce.0),
        &did_spelling::wire(creator),
    )
    .map_err(|e| ProviderError::Config(format!("policy does not compile: {e}")))
}
