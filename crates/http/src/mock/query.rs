//! Mock query executor for testing HTTP routes.

use async_trait::async_trait;
use serde_json::json;

use query::error::{QueryError, Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;

/// Mock executor for testing HTTP routes with pattern-matched query responses.
#[derive(Debug, Clone)]
pub struct MockQueryExecutor {
    schema_sdl: String,
}

/// Mock executor for testing error paths. Schema errors are optional.
#[derive(Debug, Clone)]
pub struct FailingMockExecutor {
    schema_error: Option<String>,
    finished_txn: bool,
}

impl FailingMockExecutor {
    /// Create a mock that fails schema() calls.
    pub fn with_schema_error(msg: impl Into<String>) -> Self {
        Self {
            schema_error: Some(msg.into()),
            finished_txn: false,
        }
    }

    /// Create a mock whose every transaction is already finalized.
    pub fn with_finished_txn() -> Self {
        Self {
            schema_error: None,
            finished_txn: true,
        }
    }

    fn txn_error(&self) -> TransactionError {
        match self.finished_txn {
            true => TransactionError::AlreadyFinalized("mock transaction".to_owned()),
            false => TransactionError::not_supported("mock executor does not support transactions"),
        }
    }
}

#[async_trait]
impl QueryExecutor for FailingMockExecutor {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn add_collection_as(
        &self,
        _context: &defra_core::RequestContext,
        _sdl: &str,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        Err(self.txn_error())
    }

    async fn set_migration_as(
        &self,
        _context: &defra_core::RequestContext,
        _config: &str,
    ) -> std::result::Result<String, TransactionError> {
        Err(self.txn_error())
    }

    async fn execute(&self, _request: QueryRequest) -> QueryResponse {
        QueryResponse::error("execution failed")
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Err(TransactionError::not_supported(
            "mock executor does not support transactions",
        ))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported(
            "mock executor does not support transactions",
        ))
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Err(TransactionError::not_supported(
            "mock executor does not support transactions",
        ))
    }

    async fn schema(&self) -> Result<String> {
        match &self.schema_error {
            Some(msg) => Err(QueryError::internal(msg.clone())),
            None => Ok("type Query { ping: String }".to_string()),
        }
    }
}

impl MockQueryExecutor {
    /// Create a mock executor with default schema.
    pub fn new() -> Self {
        Self {
            schema_sdl: default_mock_schema(),
        }
    }

    /// Create with custom schema.
    pub fn with_schema(schema: impl Into<String>) -> Self {
        Self {
            schema_sdl: schema.into(),
        }
    }
}

impl Default for MockQueryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn mock_collection_version(name: &str) -> schema::CollectionVersion {
    serde_json::from_value(json!({
        "Name": name,
        "VersionID": "mock-version-id",
        "CollectionID": "mock-collection-id",
    }))
    .expect("mock collection version should deserialize")
}

#[async_trait]
impl QueryExecutor for MockQueryExecutor {
    fn abandon_txn(&self, _handle: &TransactionHandle) {}

    async fn add_collection_as(
        &self,
        _context: &defra_core::RequestContext,
        _sdl: &str,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        Ok(vec![mock_collection_version("MockCollection")])
    }

    async fn get_collections_as(
        &self,
        _context: &defra_core::RequestContext,
        _selector: &schema::CollectionSelector,
    ) -> std::result::Result<Vec<schema::CollectionVersion>, TransactionError> {
        Ok(vec![mock_collection_version("MockCollection")])
    }

    async fn set_migration_as(
        &self,
        _context: &defra_core::RequestContext,
        _config: &str,
    ) -> std::result::Result<String, TransactionError> {
        Ok("mock-transform-id".to_string())
    }

    async fn execute(&self, request: QueryRequest) -> QueryResponse {
        let query = request.query.to_lowercase();

        if query.contains("users") {
            QueryResponse::success(json!({
                "users": [
                    {"_docID": "bae-123", "name": "Alice", "age": 30},
                    {"_docID": "bae-456", "name": "Bob", "age": 25}
                ]
            }))
        } else if query.contains("__schema") || query.contains("__type") {
            QueryResponse::success(json!({
                "__schema": {
                    "types": [],
                    "queryType": {"name": "Query"},
                    "mutationType": {"name": "Mutation"}
                }
            }))
        } else if query.contains("create") || query.contains("update") || query.contains("delete") {
            QueryResponse::success(json!({
                "_docID": "bae-new-789"
            }))
        } else {
            QueryResponse::success(json!({
                "data": null,
                "message": "Mock response"
            }))
        }
    }

    async fn execute_in_txn(
        &self,
        request: QueryRequest,
        _handle: &TransactionHandle,
    ) -> QueryResponse {
        self.execute(request).await
    }

    async fn begin_txn(
        &self,
        _readonly: bool,
    ) -> std::result::Result<TransactionHandle, TransactionError> {
        Ok(TransactionHandle::new("mock-txn-001".to_string()))
    }

    async fn commit_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Ok(())
    }

    async fn discard_txn(
        &self,
        _handle: &TransactionHandle,
    ) -> std::result::Result<(), TransactionError> {
        Ok(())
    }

    async fn schema(&self) -> Result<String> {
        Ok(self.schema_sdl.clone())
    }
}

pub(crate) fn default_mock_schema() -> String {
    r#"
type User {
    _docID: ID!
    name: String!
    age: Int
    email: String
}

type Query {
    users: [User!]!
    user(_docID: ID!): User
}

type Mutation {
    create_User(input: UserInput!): User!
    update_User(_docID: ID!, input: UserInput!): User!
    delete_User(_docID: ID!): User!
}

input UserInput {
    name: String!
    age: Int
    email: String
}
"#
    .to_string()
}
