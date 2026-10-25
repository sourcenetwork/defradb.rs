//! The REST routes whose work is the data path run on the worker pool.
//!
//! [`OnPool`] is the server's outermost route layer: for a route under one of
//! [`ON_POOL`]'s prefixes, the whole route (the auth middleware, the body's
//! extraction, the handler and the storage reads and writes it makes) runs as
//! one task on the database's pool, and the response comes back to the edge.
//! A request that names one document (`/collections/{name}/document/{docID}`)
//! runs on that document's worker, where every write to it runs; every other
//! request runs on the least-loaded worker.
//!
//! The document ACP routes run there too when the ACP is the node's own
//! store ([`OnPool::with_local_acp`]); a remote provider is reached through
//! the network.
//!
//! Every other route stays on the network edge: the explicit transaction
//! routes (their requests go to the worker that owns the transaction), the
//! P2P routes (they reach peers through the network), event and GraphQL
//! subscription streams, and the routes with no storage behind them. GraphQL
//! requests reach the pool with their permission check in one task (see
//! `handlers::graphql`), and the storage parts of the other edge routes reach
//! it through their operations.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::extract::{MatchedPath, Request};
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service, ServiceExt};

use crate::error::pool_failure;

/// The route prefixes, under each API prefix (`go_paths::API_PREFIXES`),
/// whose requests run on the pool.
pub const ON_POOL: [&str; 10] = [
    "/collections",
    "/index",
    "/backup",
    "/views",
    "/view",
    "/block",
    "/lens",
    "/acp/node",
    "/actions",
    "/debug",
];

/// Whether the route `path` (as matched) runs on the pool.
pub fn runs_on_pool(path: &str) -> bool {
    api_route(path, |route| {
        route == "/schema" || ON_POOL.iter().any(|prefix| under(route, prefix))
    })
}

/// Whether the route `path` (as matched) is a document ACP route.
pub fn is_acp_route(path: &str) -> bool {
    api_route(path, |route| under(route, "/acp"))
}

/// Whether `path`, below one of the API prefixes, is a route `matches`.
fn api_route(path: &str, matches: impl Fn(&str) -> bool) -> bool {
    crate::go_paths::API_PREFIXES
        .iter()
        .filter_map(|api| path.strip_prefix(api))
        .any(matches)
}

/// Whether `route` is `prefix` or below it.
fn under(route: &str, prefix: &str) -> bool {
    route
        .strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The worker a request runs on: its document's, when its path names one
/// (`/collections/{name}/document/{docID}`, keyed as a GraphQL write to it
/// is).
pub fn route_target(path: &str) -> workers::Target {
    let mut segments = path
        .split('/')
        .skip_while(|segment| *segment != "collections");
    match (segments.nth(1), segments.next(), segments.next()) {
        (Some(collection), Some("document"), Some(doc_id))
            if !collection.is_empty() && !doc_id.is_empty() =>
        {
            workers::Target::Key(query::pooled::document_key(collection, doc_id))
        }
        _ => workers::Target::LeastLoaded,
    }
}

/// The layer that runs data-path routes on `pool`.
#[derive(Clone)]
pub struct OnPool {
    pool: workers::Pool,
    local_acp: bool,
}

impl OnPool {
    /// Runs the data-path routes of the router it is laid over on `pool`.
    pub fn new(pool: workers::Pool) -> Self {
        Self {
            pool,
            local_acp: false,
        }
    }

    /// Runs the document ACP routes on the pool too: the ACP is the node's
    /// own store.
    pub fn with_local_acp(mut self) -> Self {
        self.local_acp = true;
        self
    }
}

impl<S> Layer<S> for OnPool {
    type Service = OnPoolService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OnPoolService {
            inner,
            pool: self.pool.clone(),
            local_acp: self.local_acp,
        }
    }
}

/// [`OnPool`] over one route.
#[derive(Clone)]
pub struct OnPoolService<S> {
    inner: S,
    pool: workers::Pool,
    local_acp: bool,
}

impl<S> Service<Request> for OnPoolService<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let path = request
            .extensions()
            .get::<MatchedPath>()
            .map(|matched| matched.as_str().to_owned());
        let inner = self.inner.clone();
        let local_acp = self.local_acp;
        let on_pool = |path: &String| runs_on_pool(path) || (local_acp && is_acp_route(path));
        let Some(path) = path.filter(on_pool) else {
            return Box::pin(inner.oneshot(request));
        };
        let target = route_target(request.uri().path());
        let pool = self.pool.clone();
        Box::pin(async move {
            match workers::run_on(Some(&pool), target, move || inner.oneshot(request)).await {
                Ok(response) => response,
                Err(error) => Ok(pool_failure(&path, error).into_response()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_path_routes_run_on_the_pool_and_network_routes_do_not() {
        for path in [
            "/api/v0/collections",
            "/api/v0/collections/{name}/document/{docID}",
            "/api/v0/index",
            "/api/v0/backup/export",
            "/api/v0/view/refresh",
            "/api/v0/acp/node/status",
            "/api/v0/schema",
            "/api/v1/collections",
            "/api/collections",
        ] {
            assert!(runs_on_pool(path), "{path}");
        }
        for path in [
            "/api/v0/graphql",
            "/api/v0/tx/{id}",
            "/api/v0/p2p/replicators",
            "/api/v0/acp/policy",
            "/api/v0/events",
            "/api/v0/collectionsx",
            "/health-check",
        ] {
            assert!(!runs_on_pool(path), "{path}");
        }
    }

    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use tokio::sync::Notify;

    const LIMIT: Duration = Duration::from_secs(10);

    /// A collections route on `pool` whose handler says when it starts and
    /// when it ends, and waits for `gate` in between.
    fn app(
        pool: &workers::Pool,
        started: &Arc<Notify>,
        gate: &Arc<Notify>,
        done: &Arc<Notify>,
    ) -> Router {
        let (started, gate, done) = (Arc::clone(started), Arc::clone(gate), Arc::clone(done));
        let handler = move || {
            let (started, gate, done) =
                (Arc::clone(&started), Arc::clone(&gate), Arc::clone(&done));
            async move {
                started.notify_one();
                gate.notified().await;
                done.notify_one();
                "ok"
            }
        };
        Router::new()
            .route("/api/v0/collections", get(handler))
            .route_layer(OnPool::new(pool.clone()))
    }

    fn request() -> Request {
        Request::get("/api/v0/collections")
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn a_request_that_times_out_before_its_worker_starts_it_never_runs() {
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let (release, held) = std::sync::mpsc::channel::<()>();
        // The one worker's thread is held until `release` sends.
        let hold = pool
            .submit(workers::Target::Worker(0), move || async move {
                let _ = held.recv();
            })
            .await
            .unwrap();
        let notify = || Arc::new(Notify::new());
        let (started, gate, done) = (notify(), notify(), notify());
        let timed_out = tokio::time::timeout(
            Duration::from_millis(50),
            app(&pool, &started, &gate, &done).oneshot(request()),
        )
        .await;
        assert!(timed_out.is_err(), "the held worker cannot answer");
        release.send(()).unwrap();
        hold.await.unwrap();
        // A job queued after the request's, in the same inbox, runs after
        // the request's turn.
        pool.submit(workers::Target::LeastLoaded, || async {})
            .await
            .unwrap()
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(1), started.notified())
                .await
                .is_err(),
            "the timed-out request never ran"
        );
        pool.shutdown_blocking().unwrap();
    }

    #[tokio::test]
    async fn a_request_that_times_out_while_it_runs_runs_to_its_end() {
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let notify = || Arc::new(Notify::new());
        let (started, gate, done) = (notify(), notify(), notify());
        let mut response = Box::pin(app(&pool, &started, &gate, &done).oneshot(request()));
        tokio::select! {
            _ = &mut response => panic!("the request ended before its gate opened"),
            _ = started.notified() => {}
        }
        drop(response);
        gate.notify_one();
        tokio::time::timeout(LIMIT, done.notified())
            .await
            .expect("the running request finished");
        pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_request_naming_one_document_goes_to_its_worker() {
        assert_eq!(
            route_target("/api/v0/collections/User/document/bae-1"),
            workers::Target::Key(query::pooled::document_key("User", "bae-1"))
        );
        assert_eq!(
            route_target("/api/v0/collections/User"),
            workers::Target::LeastLoaded
        );
    }
}
