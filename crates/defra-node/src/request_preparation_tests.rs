use async_trait::async_trait;
use query::error::{Result, TransactionError};
use query::{QueryExecutor, QueryRequest, QueryResponse, TransactionHandle};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
struct ConflictExecutor {
    preparations: AtomicUsize,
    auto_commit_attempts: AtomicUsize,
    explicit_txn_attempts: AtomicUsize,
}

#[async_trait]
impl QueryExecutor for ConflictExecutor {
    async fn prepare_request(
        &self,
        _request: &QueryRequest,
    ) -> Result<Option<Arc<query::prepared::PreparedMutations>>> {
        self.preparations.fetch_add(1, Ordering::SeqCst);
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

#[tokio::test]
async fn embedded_retry_prepares_once_before_all_attempts() {
    let executor = ConflictExecutor::default();
    let response = super::execute_autocommit_request(
        &executor,
        QueryRequest::new("{ probe }"),
        Some(super::ExecuteRetryPolicy::new(
            3,
            std::time::Duration::ZERO,
            std::time::Duration::ZERO,
        )),
    )
    .await;
    assert!(!response.has_errors());
    assert_eq!(executor.preparations.load(Ordering::SeqCst), 1);
    assert_eq!(executor.auto_commit_attempts.load(Ordering::SeqCst), 3);
}
