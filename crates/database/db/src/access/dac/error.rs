use crate::error::Error;

/// Why a DAC policy operation was refused or failed. Each refusal Go's
/// DefraDB has carries Go's text.
#[derive(Debug, thiserror::Error)]
pub enum DacPolicyError {
    /// The node runs without document ACP.
    #[error("operation requires ACP, but ACP not available")]
    AcpNotAvailable,
    /// No identity acts in the request: a policy needs a creator.
    #[error("policy creator can not be empty")]
    CreatorEmpty,
    /// The policy text is empty.
    #[error("policy data can not be empty")]
    DataEmpty,
    /// The ACP refused or failed to store the policy.
    #[error("failed to add policy with acp: {message}. Type: {acp}, CreatorID: {creator}")]
    AddFailed {
        message: String,
        acp: &'static str,
        creator: String,
    },
    /// A counter makes a local ACP's policy ID; the remote ACP (vera.rs)
    /// derives its IDs from the definition, the creator and a nonce.
    #[error("a policy counter makes a local ACP's policy ID; the remote ACP derives its IDs from a nonce")]
    CounterNeedsLocalAcp,
    /// A nonce makes a remote ACP's policy ID; a local ACP numbers its
    /// policies, as Go's does.
    #[error("a policy nonce makes a remote ACP's policy ID; the local ACP numbers its policies")]
    NonceNeedsRemoteAcp,
    /// The policy store could not be read for policy `policy_id`.
    #[error("policy validation failed with acp: {message}. PolicyID: {policy_id}")]
    Lookup { message: String, policy_id: String },
    /// The policy store could not be listed.
    #[error("failed to list policies: {0}")]
    List(String),
    /// The policy or resource a collection names does not satisfy Go's
    /// resource interface.
    #[error("{0}")]
    ResourceInterface(String),
    /// The node's ACP has no light client.
    #[error("ACP light client status is not available for this ACP backend")]
    NoLightClient,
    /// The light client could not report its status.
    #[error("failed to get ACP light client status: {0}")]
    LightClient(String),
    /// The caller may not run the operation.
    #[error(transparent)]
    Db(#[from] Error),
}
