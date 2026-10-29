//! How the context of one HTTP request reaches the handler serving it and the
//! operations that handler calls.
//!
//! The auth middleware builds the request's [`RequestContext`] (who acts in
//! it) and stores it in the request's extensions. A handler takes it as a
//! [`CallerContext`] and binds each operations handle it calls to it with
//! [`bound`], so an implementation that reads the acting identity (node access
//! control, the creator of what it writes) reads it from there and never from
//! the thread or task that happens to run it.

use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use defra_core::RequestContext;
use query::rest::RestOperations;

use crate::router::{
    AcpOperations, BackupOperations, BlockOperations, CollectionManagementOperations,
    CollectionOperations, DocumentAcpOperations, EncryptedIndexOperations, IndexOperations,
    P2POperations, ViewOperations,
};

/// The context of the request a handler serves: the one the auth middleware
/// built, or [`RequestContext::ambient`] on a route served without it.
#[derive(Debug, Clone)]
pub struct CallerContext(pub RequestContext);

impl<S> FromRequestParts<S> for CallerContext
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<RequestContext>()
                .cloned()
                .unwrap_or_else(RequestContext::ambient),
        ))
    }
}

/// An operations handle that can serve one request.
pub trait ForRequest {
    /// This handle bound to the request `context` serves, or `None` when the
    /// implementation reads nothing of a request.
    fn for_request(&self, context: &RequestContext) -> Option<Arc<Self>>;
}

macro_rules! for_request {
    ($($operations:ident),* $(,)?) => {
        $(
            impl ForRequest for dyn $operations {
                fn for_request(&self, context: &RequestContext) -> Option<Arc<Self>> {
                    self.bind_request(context)
                }
            }
        )*
    };
}

for_request!(
    AcpOperations,
    BackupOperations,
    BlockOperations,
    CollectionManagementOperations,
    DocumentAcpOperations,
    EncryptedIndexOperations,
    IndexOperations,
    P2POperations,
    RestOperations,
    CollectionOperations,
    ViewOperations,
);

/// `handle` serving the request `context` serves.
pub fn bound<T>(handle: &Arc<T>, context: &RequestContext) -> Arc<T>
where
    T: ForRequest + ?Sized,
{
    handle
        .for_request(context)
        .unwrap_or_else(|| Arc::clone(handle))
}
