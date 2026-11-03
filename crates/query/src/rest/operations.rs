//! Production implementation of REST operations using QueryRunner.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;
use identity::Did;
use serde_json::Value as JsonValue;

use crate::executor::{QueryExecutor, QueryRequest, TXN_CONFLICT_ERROR_CODE};
use crate::fetcher::{CollectionProvider, DocFetcher};
use crate::runner::QueryRunner;
use crate::txn::{TransactionHandle, TransactionRegistry};

use super::error::{RestError, RestResult};
use super::gql;
use super::trait_def::{CollectionDocIdsPage, CollectionDocIdsPagination, RestOperations};

/// Production implementation of REST operations using QueryRunner.
///
/// This wraps a QueryRunner and translates REST reads into GraphQL queries.
pub struct RestOperationsImpl<F: DocFetcher, R: TransactionRegistry> {
    runner: Arc<QueryRunner<F, R>>,
    transaction: Option<(TransactionHandle, Arc<dyn CollectionProvider>)>,
}

impl<F: DocFetcher + 'static, R: TransactionRegistry> RestOperationsImpl<F, R> {
    /// Create a new REST operations implementation.
    pub fn new(runner: Arc<QueryRunner<F, R>>) -> Self {
        Self {
            runner,
            transaction: None,
        }
    }

    async fn has_collection(&self, name: &str) -> RestResult<bool> {
        match &self.transaction {
            Some((_, provider)) => Ok(provider.get_collection(name).await?.is_some()),
            None => Ok(self.runner.has_collection(name).await?),
        }
    }

    async fn get_collection(&self, name: &str) -> RestResult<Arc<schema::CollectionVersion>> {
        match &self.transaction {
            Some((_, provider)) => provider
                .get_collection(name)
                .await?
                .ok_or_else(|| RestError::collection_not_found(name)),
            None => Ok(self.runner.get_collection(name).await?),
        }
    }

    async fn execute(&self, query: &str, identity: Option<&Did>) -> RestResult<JsonValue> {
        if let Some((handle, _)) = &self.transaction {
            let response = self
                .runner
                .execute_in_txn(
                    QueryRequest::new(query)
                        .with_identity(identity.cloned())
                        .with_context(self.runner.context().into_owned()),
                    handle,
                )
                .await;
            if let Some(error) = response.errors.first() {
                if error
                    .extensions
                    .as_ref()
                    .is_some_and(|extension| extension.code == TXN_CONFLICT_ERROR_CODE)
                {
                    return Err(RestError::TransactionConflict);
                }
                return Err(RestError::internal(error.message.clone()));
            }
            return response
                .data
                .ok_or_else(|| RestError::internal("transaction returned no data"));
        }
        Ok(self
            .runner
            .execute_query_with_identity(query, identity.cloned())
            .await?)
    }

    fn extract_doc_ids(&self, result: &JsonValue, collection: &str) -> RestResult<Vec<String>> {
        let docs = result.get(collection).ok_or_else(|| {
            tracing::warn!(
                collection = %collection,
                result = ?result,
                "Query result missing collection key"
            );
            RestError::internal(format!("query result missing key '{}'", collection))
        })?;

        let arr = docs.as_array().ok_or_else(|| {
            tracing::warn!(
                collection = %collection,
                result = ?result,
                "Query result collection is not an array"
            );
            RestError::internal(format!(
                "expected array for collection '{}', got {}",
                collection,
                match docs {
                    JsonValue::Null => "null",
                    JsonValue::Bool(_) => "boolean",
                    JsonValue::Number(_) => "number",
                    JsonValue::String(_) => "string",
                    JsonValue::Object(_) => "object",
                    JsonValue::Array(_) => "array",
                }
            ))
        })?;

        Ok(arr
            .iter()
            .filter_map(|doc| doc.get("_docID").and_then(|id| id.as_str()))
            .map(String::from)
            .collect())
    }

    fn extract_doc_id_total(&self, result: &JsonValue, collection: &str) -> RestResult<usize> {
        let total = result
            .get("total")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                tracing::warn!(
                    collection = %collection,
                    result = ?result,
                    "Count query result missing numeric total"
                );
                RestError::internal(format!(
                    "expected numeric total for collection '{}'",
                    collection
                ))
            })?;

        usize::try_from(total).map_err(|_| {
            RestError::internal(format!(
                "document count for collection '{}' exceeds platform limits",
                collection
            ))
        })
    }

    async fn fetch_full_document(
        &self,
        collection: &str,
        doc_id: &str,
        identity: Option<&Did>,
    ) -> RestResult<Option<JsonValue>> {
        let coll = self
            .get_collection(collection)
            .await
            .map_err(|e| RestError::internal(e.to_string()))?;
        let fields: Vec<&str> = std::iter::once("_docID")
            .chain(coll.fields.iter().filter_map(|f| {
                if f.name == "_docID" || !f.kind.is_scalar() {
                    None
                } else {
                    Some(f.name.as_str())
                }
            }))
            .collect();
        let selection = fields.join(" ");

        let query = format!(
            r#"{{ {collection}(docID: "{doc_id}") {{ {selection} }} }}"#,
            collection = collection,
            doc_id = doc_id,
            selection = selection
        );

        let result = self.execute(&query, identity).await?;

        let doc = result
            .get(collection)
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .cloned();

        Ok(doc)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<F: DocFetcher + 'static, R: TransactionRegistry + 'static> RestOperations
    for RestOperationsImpl<F, R>
{
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn RestOperations>> {
        Some(Arc::new(Self {
            runner: Arc::new(self.runner.for_request(context.clone())),
            transaction: self.transaction.clone(),
        }))
    }

    fn with_transaction(&self, handle: TransactionHandle) -> RestResult<Arc<dyn RestOperations>> {
        let provider = self.runner.transaction_collection_provider(&handle)?;
        Ok(Arc::new(Self {
            runner: self.runner.clone(),
            transaction: Some((handle, provider)),
        }))
    }

    async fn list_collections(&self) -> RestResult<Vec<String>> {
        if let Some((_, provider)) = &self.transaction {
            let mut names = provider.list_collections().await?;
            names.sort();
            return Ok(names);
        }
        self.runner
            .collection_names()
            .await
            .map_err(|e| RestError::internal(e.to_string()))
    }

    async fn get_collection_doc_ids(
        &self,
        collection: &str,
        identity: Option<&Did>,
    ) -> RestResult<Vec<String>> {
        if !self
            .has_collection(collection)
            .await
            .map_err(|e| RestError::internal(e.to_string()))?
        {
            return Err(RestError::collection_not_found(collection));
        }

        let query = gql::build_list_ids_query(collection, None);
        let result = self.execute(&query, identity).await?;
        self.extract_doc_ids(&result, collection)
    }

    async fn get_collection_doc_ids_page(
        &self,
        collection: &str,
        pagination: CollectionDocIdsPagination,
        identity: Option<&Did>,
    ) -> RestResult<CollectionDocIdsPage> {
        if !self
            .has_collection(collection)
            .await
            .map_err(|e| RestError::internal(e.to_string()))?
        {
            return Err(RestError::collection_not_found(collection));
        }

        let count_query = gql::build_count_query(collection);
        let count_result = self.execute(&count_query, identity).await?;
        let total = self.extract_doc_id_total(&count_result, collection)?;

        if pagination.offset >= total {
            return Ok(CollectionDocIdsPage {
                doc_ids: Vec::new(),
                total,
                limit: pagination.limit,
                offset: pagination.offset,
            });
        }

        let query = gql::build_list_ids_query(collection, Some(pagination));
        let result = self.execute(&query, identity).await?;
        let doc_ids = self.extract_doc_ids(&result, collection)?;

        Ok(CollectionDocIdsPage {
            doc_ids,
            total,
            limit: pagination.limit,
            offset: pagination.offset,
        })
    }

    async fn get_document(
        &self,
        collection: &str,
        doc_id: &str,
        identity: Option<&Did>,
    ) -> RestResult<Option<JsonValue>> {
        if !self
            .has_collection(collection)
            .await
            .map_err(|e| RestError::internal(e.to_string()))?
        {
            return Err(RestError::collection_not_found(collection));
        }

        self.fetch_full_document(collection, doc_id, identity).await
    }
}
