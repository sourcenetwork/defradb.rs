//! The digest a request-bound delegation names, as the provider reports its
//! failures.

use crate::provider::ProviderError;
use crate::PolicyCommand;

/// [`vera_delegation::request_digest`], failing as a [`ProviderError`].
pub fn request_digest(command: &PolicyCommand<'_>) -> Result<[u8; 32], ProviderError> {
    Ok(vera_delegation::request_digest(command)?)
}
