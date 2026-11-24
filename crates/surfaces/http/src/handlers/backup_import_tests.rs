//! The import route hands its backend this server's request path: a write
//! the backend sends through the host reaches the server's executor.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use query::error::{Result, TransactionError};
use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
use tower::ServiceExt;

use crate::mock::MockQueryExecutor;
use crate::router::{BackupOperations, ImportResult};
use crate::server::{Server, ServerConfig};

/// The mock executor, counting the requests it executes and noting whether
/// one ran on a database worker.
struct Counting {
    inner: MockQueryExecutor,
    executed: Arc<AtomicUsize>,
    on_worker: Arc<AtomicBool>,
}

#[async_trait]
impl QueryExecutor for Counting {
    fn abandon_txn(&self, handle: &TransactionHandle) {
        self.inner.abandon_txn(handle)
    }

    async fn execute(&self, request: QueryRequest) -> QueryResponse {
        self.executed.fetch_add(1, Ordering::SeqCst);
        self.on_worker
            .fetch_or(workers::on_worker(), Ordering::SeqCst);
        self.inner.execute(request).await
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        handle: &TransactionHandle,
    ) -> QueryResponse {
        self.inner.execute_in_txn(request, handle).await
    }

    async fn begin_txn(
        &self,
        readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        self.inner.begin_txn(readonly).await
    }

    async fn commit_txn(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.inner.commit_txn(handle).await
    }

    async fn discard_txn(
        &self,
        handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        self.inner.discard_txn(handle).await
    }

    async fn schema(&self) -> Result<String> {
        self.inner.schema().await
    }
}

/// A backend whose import sends one request through the host it is given,
/// noting whether the import itself ran on a database worker.
#[derive(Default)]
struct OneWrite {
    on_worker: Arc<AtomicBool>,
}

#[async_trait]
impl BackupOperations for OneWrite {
    async fn export(
        &self,
        _collections: Option<Vec<String>>,
        _pretty: bool,
    ) -> std::result::Result<String, String> {
        Ok("{}".into())
    }

    async fn import(
        &self,
        _data: &str,
        host: &dyn db::backup::ImportHost,
    ) -> std::result::Result<ImportResult, String> {
        self.on_worker.store(workers::on_worker(), Ordering::SeqCst);
        host.run(QueryRequest::new("{ users { name } }")).await;
        Ok(ImportResult {
            documents_imported: 1,
            documents_skipped: 0,
            collections_affected: vec!["User".into()],
            errors: Vec::new(),
        })
    }
}

fn import_request() -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/api/v0/backup/import")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"User": [{"name": "John"}]}"#))
        .unwrap()
}

#[nuclei::test]
async fn an_import_writes_through_this_servers_executor() {
    let executed = Arc::new(AtomicUsize::new(0));
    let executor = Counting {
        inner: MockQueryExecutor::default(),
        executed: Arc::clone(&executed),
        on_worker: Arc::default(),
    };
    let router = Server::with_config(executor, ServerConfig::default(), edge::testing::edge())
        .with_backup_arc(Arc::new(OneWrite::default()))
        .with_dev_mode(true)
        .router()
        .expect("router builds");

    let response = router.oneshot(import_request()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(executed.load(Ordering::SeqCst), 1);
}

/// On a server with a worker pool the import route stays on the edge: the
/// driver runs off the workers, and each write it sends reaches a worker
/// through the request path (`INV_DriverOnCaller`).
#[nuclei::test]
async fn the_import_driver_runs_off_the_workers_and_its_writes_on_them() {
    let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
    let wrote_on_worker = Arc::new(AtomicBool::new(false));
    let executor = Counting {
        inner: MockQueryExecutor::default(),
        executed: Arc::default(),
        on_worker: Arc::clone(&wrote_on_worker),
    };
    let backend = Arc::new(OneWrite::default());
    let router = Server::with_config(executor, ServerConfig::default(), edge::testing::edge())
        .with_backup_arc(backend.clone())
        .with_dev_mode(true)
        .with_pool(Some(pool.clone()))
        .router()
        .expect("router builds");

    let response = router.oneshot(import_request()).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !backend.on_worker.load(Ordering::SeqCst),
        "the import driver ran on a database worker"
    );
    assert!(
        wrote_on_worker.load(Ordering::SeqCst),
        "the import's write did not reach a worker"
    );
    pool.shutdown_blocking().unwrap();
}
