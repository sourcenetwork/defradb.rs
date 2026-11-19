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

use crate::identity_extractor::{caller_from_headers, ExtractIdentity, MiddlewareIdentity};
use crate::nac_guard::require_permissions;
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
    let caller = match permission {
        RoutePermission::Exempt => None,
        RoutePermission::IdentityOnly | RoutePermission::Dynamic | RoutePermission::Required(_) => {
            match caller_from_headers(request.headers(), state.doc_acp.as_deref()) {
                Ok(did) => did,
                Err(err) => return err.into_response(),
            }
        }
    };

    let (acting_did, context) = bind_caller(&mut request, caller, !is_exempt);
    let execution = async move {
        let identity = ExtractIdentity::from_did(acting_did);
        // `get-collection` first on the collection handler routes, then the
        // route's own permission, decided in one call so the edge crosses
        // onto the pool once.
        let both;
        let checks: &[NodePermission] = match (reads_collection_first, &permission) {
            (true, RoutePermission::Required(own)) => {
                both = [NodePermission::CollectionGet, *own];
                &both
            }
            (true, _) => &[NodePermission::CollectionGet],
            (false, RoutePermission::Required(own)) => std::slice::from_ref(own),
            (false, _) => &[],
        };
        if let Err(err) = require_permissions(&state, &identity, checks).await {
            return err.into_response();
        }
        next.run(request).await
    };
    context.mirror(execution).await
}

fn bind_caller(
    request: &mut Request,
    caller: Option<acp::AuthenticatedCaller>,
    publish_identity: bool,
) -> (Option<identity::Did>, RequestContext) {
    let expiration = caller.as_ref().map(|caller| caller.expires_at);
    let acting_did = caller.map(|caller| caller.did);
    if publish_identity {
        request
            .extensions_mut()
            .insert(MiddlewareIdentity(acting_did.clone()));
    }
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
        .with_bearer(scoped, bearer)
        .with_authorization_expiration(expiration);
    request.extensions_mut().insert(context.clone());
    (acting_did, context)
}

/// Authenticate application routes with this node's credential verifier.
/// The application owner decides operation permissions; a verified caller is
/// never itself a document or node-access grant.
pub async fn authenticate_application(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let caller = match caller_from_headers(request.headers(), state.doc_acp.as_deref()) {
        Ok(caller) => caller,
        Err(error) => return error.into_response(),
    };
    let (_, context) = bind_caller(&mut request, caller, true);
    context.mirror(next.run(request)).await
}
