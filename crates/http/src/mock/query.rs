//! Mock query executor for testing HTTP routes.

use std::sync::Arc;

use async_trait::async_trait;
use kovan::Atom;
use serde_json::json;

use crate::mock::nac::MockNodeAcpOperations;
use defra_core::RequestContext;
use query::documents::Filter;
use query::error::{QueryError, Result, TransactionError};
use query::executor::{QueryExecutor, QueryRequest, QueryResponse};
use query::TransactionHandle;

/// A truncate the mock was asked for: the collection and its filter.
type TruncateCall = (String, Option<Filter>);

/// Mock executor for testing HTTP routes with pattern-matched query responses.
#[derive(Debug, Clone)]
pub struct MockQueryExecutor {
    schema_sdl: String,
    /// Each truncate asked for: the collection and filter.
    truncated: Arc<Atom<Vec<TruncateCall>>>,
    /// Backs the node access control executor methods when set.
    nac: Option<Arc<MockNodeAcpOperations>>,
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

/// Map a mock NAC failure onto the executor error that the HTTP layer
/// translates through `http_error_from_backend_message` (BadRequest unless
/// the message classifies otherwise).
fn nac_error(message: String) -> TransactionError {
    TransactionError::execution(message)
}

impl MockQueryExecutor {
    /// Create a mock executor with default schema.
    pub fn new() -> Self {
        Self::with_schema(default_mock_schema())
    }

    /// Create with custom schema.
    pub fn with_schema(schema: impl Into<String>) -> Self {
        Self {
            schema_sdl: schema.into(),
            truncated: Arc::default(),
            nac: None,
        }
    }

    /// Create with a NAC backend: the node access control executor methods
    /// delegate to it instead of refusing.
    pub fn with_nac(nac: Arc<MockNodeAcpOperations>) -> Self {
        Self {
            schema_sdl: default_mock_schema(),
            truncated: Arc::default(),
            nac: Some(nac),
        }
    }

    /// The truncates asked for, oldest first.
    pub fn truncated_collections(&self) -> Vec<TruncateCall> {
        self.truncated.load_clone()
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

    async fn truncate_collection_as(
        &self,
        _context: &defra_core::RequestContext,
        name: &str,
        filter: Option<&Filter>,
    ) -> std::result::Result<(), TransactionError> {
        crate::mock::update_vec(&self.truncated, |calls| {
            calls.push((name.to_owned(), filter.cloned()))
        });
        Ok(())
    }

    async fn add_dac_policy_as(
        &self,
        _context: &defra_core::RequestContext,
        _policy: &str,
    ) -> std::result::Result<String, TransactionError> {
        Ok("mock-policy-id".to_owned())
    }

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


    async fn nac_status_as(
        &self,
        _context: &RequestContext,
    ) -> std::result::Result<acp::nac::NacStatus, TransactionError> {
        match &self.nac {
            Some(nac) => Ok(nac.status()),
            None => Err(TransactionError::not_supported(
                "node access control is not supported by this executor",
            )),
        }
    }

    async fn disable_nac_as(
        &self,
        _context: &RequestContext,
    ) -> std::result::Result<(), TransactionError> {
        match &self.nac {
            Some(nac) => nac.disable_nac().map_err(nac_error),
            None => Err(TransactionError::not_supported(
                "node access control is not supported by this executor",
            )),
        }
    }

    async fn re_enable_nac_as(
        &self,
        _context: &RequestContext,
    ) -> std::result::Result<(), TransactionError> {
        match &self.nac {
            Some(nac) => nac.re_enable_nac().map_err(nac_error),
            None => Err(TransactionError::not_supported(
                "node access control is not supported by this executor",
            )),
        }
    }

    async fn add_nac_actor_relationship_as(
        &self,
        _context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> std::result::Result<bool, TransactionError> {
        match &self.nac {
            Some(nac) => {
                let target = target_actor
                    .parse()
                    .map_err(|_| TransactionError::execution(format!(
                        "invalid TargetActor DID: {target_actor}"
                    )))?;
                nac.add_relationship(relation, &target).await.map_err(nac_error)
            }
            None => Err(TransactionError::not_supported(
                "node access control is not supported by this executor",
            )),
        }
    }

    async fn delete_nac_actor_relationship_as(
        &self,
        _context: &RequestContext,
        relation: &str,
        target_actor: &str,
    ) -> std::result::Result<bool, TransactionError> {
        match &self.nac {
            Some(nac) => {
                let target = target_actor
                    .parse()
                    .map_err(|_| TransactionError::execution(format!(
                        "invalid TargetActor DID: {target_actor}"
                    )))?;
                nac.remove_relationship(relation, &target).await.map_err(nac_error)
            }
            None => Err(TransactionError::not_supported(
                "node access control is not supported by this executor",
            )),
        }
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
