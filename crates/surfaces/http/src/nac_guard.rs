//! NAC (Node Access Control) permission guard helpers.
//!
//! This module provides utility functions for HTTP handlers to enforce
//! NAC permission checks before performing operations.
//!
//! # Error Handling
//!
//! This module returns `401 Unauthorized` for NAC permission denials to match
//! Go DefraDB's behavior. Go's `CollectionMiddleware` returns 401 when it
//! detects `ErrNotAuthorizedToPerformOperation`.

use identity::Did;

use crate::error::HttpError;
use crate::identity_extractor::ExtractIdentity;
use crate::router::{AppState, NodePermission, PermissionCheck};

/// Check if an identity has a specific NAC permission.
///
/// Returns `Ok(())` if the permission is granted, or an appropriate error:
/// - `HttpError::Unauthorized` (401) if the identity lacks the required permission
/// - `HttpError::Unauthorized` (401) if authentication is required but not provided
///
/// If NAC is not configured on the server, all permissions are allowed.
///
/// # Anonymous / Wildcard Fallback
///
/// When no identity is provided (anonymous), checks the wildcard DID (`*`)
/// for the requested permission before rejecting. This matches FFI behavior
/// in `nac_check.rs` where anonymous requests check `Did::wildcard()`.
///
/// # Go DefraDB Compatibility
///
/// Returns 401 Unauthorized to match Go DefraDB's CollectionMiddleware which
/// returns `http.StatusUnauthorized` for `ErrNotAuthorizedToPerformOperation`.
pub async fn require_permission(
    state: &AppState,
    identity: &ExtractIdentity,
    permission: NodePermission,
) -> Result<(), HttpError> {
    require_permissions(state, identity, &[permission]).await
}

/// [`require_permission`] for each of `permissions` in order, decided in one
/// [`check_permissions`](crate::router::NodeAcpOperations::check_permissions)
/// call: the first one missing is the refusal, as checking them one after
/// another would answer.
pub async fn require_permissions(
    state: &AppState,
    identity: &ExtractIdentity,
    permissions: &[NodePermission],
) -> Result<(), HttpError> {
    // If NAC is not configured, allow all operations
    let Some(nac) = &state.nac else {
        return Ok(());
    };
    if permissions.is_empty() {
        return Ok(());
    }

    // Anonymous request: check if wildcard DID has the permission.
    // Matches FFI nac_check.rs behavior where empty/null identity
    // checks Did::wildcard() before rejecting.
    let anonymous = identity.did().is_none();
    let did = identity.did().cloned().unwrap_or_else(Did::wildcard);

    match nac.check_permissions(&did, permissions).await {
        PermissionCheck::Granted => Ok(()),
        PermissionCheck::Denied(permission) => {
            Err(HttpError::Unauthorized(not_authorized(permission)))
        }
        // A wildcard check that fails refuses the anonymous caller.
        PermissionCheck::Failed(permission, _) if anonymous => {
            Err(HttpError::Unauthorized(not_authorized(permission)))
        }
        PermissionCheck::Failed(permission, e) => {
            tracing::error!(error = %e, ?permission, "NAC permission check failed");
            Err(HttpError::Internal("permission check failed".into()))
        }
    }
}

/// Go's `ErrNotAuthorizedToPerformOperation`, which names the permission.
fn not_authorized(permission: NodePermission) -> String {
    format!(
        "not authorized to perform operation. Permission: {}",
        permission.as_str()
    )
}

/// Get the DID from an identity extractor, returning an error if not authenticated.
///
/// Helper for handlers that need the DID after a permission check.
pub fn require_identity(identity: &ExtractIdentity) -> Result<&Did, HttpError> {
    identity
        .did()
        .ok_or_else(|| HttpError::Unauthorized("authentication required".into()))
}
