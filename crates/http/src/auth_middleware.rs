//! Global auth middleware for consistent access control enforcement.
//!
//! Intercepts every matched request, parses the identity from the
//! Authorization header, and enforces the route's permission requirement
//! before the handler runs. Existing per-handler `require_permission()` calls
//! remain as a redundant safety net.
//!
//! It also builds the request's [`RequestContext`] with the parsed identity
//! acting in it and stores it in the request's extensions, where the handlers
//! take it (see [`crate::request_scope`]) so DB-layer NAC checks resolve who
//! is acting, reusing the single parse done here rather than a separate
//! middleware that re-verified the JWT.

use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};

use defra_core::RequestContext;

use crate::identity_extractor::{parse_identity_from_headers, ExtractIdentity, MiddlewareIdentity};
use crate::nac_guard::require_permission;
use crate::route_permissions::{self, route_permission, RoutePermission};
use crate::router::{AppState, NodePermission};

/// Auth middleware that enforces route-level permissions.
///
/// Applied via `Router::route_layer()` so it runs after routing
/// (MatchedPath is available) but before the handler.
pub async fn auth_middleware(
    State(state): State<AppState>,
    matched_path: Option<MatchedPath>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = matched_path
        .as_ref()
        .map(|mp| mp.as_str())
        .unwrap_or_else(|| {
            tracing::warn!("MatchedPath unavailable in auth middleware — allowing request");
            ""
        });

    let permission = route_permission(path, &method);
    let is_exempt = matches!(permission, RoutePermission::Exempt);
    let reads_collection_first = route_permissions::reads_collection_first(path, &method);

    // Parse the caller's identity once. The same value is reused below both to
    // pre-populate the handler extension and to bind the request-scoped NAC
    // context, so the JWT is verified only once per request.
    let acting_did = match permission {
        RoutePermission::Exempt => None,
        RoutePermission::IdentityOnly | RoutePermission::Dynamic | RoutePermission::Required(_) => {
            match parse_identity_from_headers(request.headers()) {
                Ok(did) => did,
                Err(err) => return err.into_response(),
            }
        }
    };

    if !is_exempt {
        request
            .extensions_mut()
            .insert(MiddlewareIdentity(acting_did.clone()));
    }

    // The request's context carries the caller's DID to the DB-layer NAC
    // checks the handlers reach, mirroring Go's `identity.FromContext`, and
    // this request's verified token from its header, the one a remote ACP
    // acts on the caller's behalf with. Never publish the token in the
    // process-wide DID map: concurrent requests can share a DID while
    // carrying different authorization tokens.
    let scoped = acting_did.as_ref().map(ToString::to_string);
    let bearer = acting_did.as_ref().and_then(|_| {
        request
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
            .map(|token| token.trim().to_owned())
    });
    let context = RequestContext::default()
        .with_identity(scoped.clone())
        .with_bearer(scoped, bearer);
    request.extensions_mut().insert(context.clone());
    let execution = async move {
        let identity = ExtractIdentity::from_did(acting_did);
        if reads_collection_first {
            if let Err(err) =
                require_permission(&state, &identity, NodePermission::CollectionGet).await
            {
                return err.into_response();
            }
        }
        if let RoutePermission::Required(perm) = permission {
            if let Err(err) = require_permission(&state, &identity, perm).await {
                return err.into_response();
            }
        }
        next.run(request).await
    };
    context.mirror(execution).await
}
