use async_trait::async_trait;
use query::error::{Result, TransactionError};
use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
struct ConflictExecutor {
    preparations: AtomicUsize,
    auto_commit_attempts: AtomicUsize,
    explicit_txn_attempts: AtomicUsize,
    preparation_on_worker: AtomicBool,
    execution_on_worker: AtomicBool,
}

#[async_trait]
impl QueryExecutor for ConflictExecutor {
    async fn prepare_request(
        &self,
        _request: &QueryRequest,
    ) -> Result<Option<Arc<query::prepared::PreparedMutations>>> {
        self.preparations.fetch_add(1, Ordering::SeqCst);
        self.preparation_on_worker
            .store(workers::on_worker(), Ordering::SeqCst);
        Ok(None)
    }

    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        panic!("retry attempts must reuse prepared inputs")
    }

    async fn execute_prepared(
        &self,
        _request: QueryRequest,
        _prepared: Option<Arc<query::prepared::PreparedMutations>>,
    ) -> QueryResponse {
        assert_eq!(self.preparations.load(Ordering::SeqCst), 1);
        self.execution_on_worker
            .store(workers::on_worker(), Ordering::SeqCst);
        if self.auto_commit_attempts.fetch_add(1, Ordering::SeqCst) < 2 {
            QueryResponse::transaction_conflict("transaction conflict")
        } else {
            QueryResponse::success(serde_json::json!({"_docID": "doc"}))
        }
    }

    async fn execute_in_txn(
        &self,
        _request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.explicit_txn_attempts.fetch_add(1, Ordering::SeqCst);
        QueryResponse::transaction_conflict("transaction conflict")
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn rollback_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported("test executor"))
    }

    async fn schema(&self) -> Result<String> {
        Ok(String::new())
    }
}

#[test]
fn embedded_returns_conflict_after_one_prepared_attempt() {
    workers::block_on(async {
        let executor = ConflictExecutor::default();
        let response =
            super::execute_autocommit_request(&executor, QueryRequest::new("{ probe }")).await;
        assert!(response.is_transaction_conflict());
        assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
        assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

#[test]
fn autocommit_prepares_on_caller_and_routes_execution_to_node_worker() {
    workers::block_on(async {
        let executor = Arc::new(ConflictExecutor::default());
        let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
        let pooled = query::pooled::PooledExecutor::new(executor.clone(), pool.clone());

        let response =
            super::execute_autocommit_request(&pooled, QueryRequest::new("{ probe }")).await;

        assert!(response.is_transaction_conflict());
        assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
        assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 1);
        assert!(
            !executor.preparation_on_worker.load(Ordering::SeqCst),
            "remote preparation stays on the caller, outside worker execution"
        );
        assert!(
            executor.execution_on_worker.load(Ordering::SeqCst),
            "the prepared autocommit attempt runs on the configured worker pool"
        );

        pool.stop().await.unwrap();
    })
    .unwrap()
}
