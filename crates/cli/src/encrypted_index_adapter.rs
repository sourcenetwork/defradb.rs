use std::borrow::Cow;
use std::sync::Arc;

use acp::nac::NodePermission;
use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{EncryptedIndexInfo, EncryptedIndexOperations};
use query::QueryExecutor;
use storage::corekv::Store;

/// Adapter that implements EncryptedIndexOperations using database SE support.
/// A request in a transaction runs in it, through `runner`.
pub struct EncryptedIndexAdapter<S>
where
    S: Store,
{
    database: Arc<db::DB<S>>,
    runner: Arc<dyn QueryExecutor>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> EncryptedIndexAdapter<S>
where
    S: Store + 'static,
{
    pub fn new_arc(
        database: Arc<db::DB<S>>,
        runner: Arc<dyn QueryExecutor>,
    ) -> Arc<dyn EncryptedIndexOperations> {
        Arc::new(Self {
            database,
            runner,
            context: None,
        })
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }

    /// The context of the request when it runs in a transaction.
    fn in_txn(&self) -> Option<Cow<'_, RequestContext>> {
        let context = self.context();
        context.txn().is_some().then_some(context)
    }

    /// The NAC checks an encrypted index operation on a collection makes
    /// outside a transaction: the caller may read collections and may do
    /// `permission`. In a transaction the registry checks both there.
    async fn allowed(&self, permission: NodePermission) -> Result<(), String> {
        let context = self.context();
        for needed in [NodePermission::CollectionGet, permission] {
            self.database
                .check_node_access_as(&context, needed)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn equality(collection: &str, field_name: &str) -> EncryptedIndexInfo {
    EncryptedIndexInfo {
        collection: collection.to_string(),
        field_name: field_name.to_string(),
        index_type: "equality".to_string(),
    }
}

#[async_trait]
impl<S> EncryptedIndexOperations for EncryptedIndexAdapter<S>
where
    S: Store + 'static,
{
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn EncryptedIndexOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            runner: Arc::clone(&self.runner),
            context: Some(context.clone()),
        }))
    }

    async fn add_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexInfo, String> {
        if let Some(context) = self.in_txn() {
            self.runner
                .add_encrypted_index_in_txn_as(&context, collection, field_name)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            self.allowed(NodePermission::EncryptedIndexAdd).await?;
            self.database
                .add_encrypted_index(collection, field_name)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(equality(collection, field_name))
    }

    async fn list_encrypted_indexes(
        &self,
        collection: Option<&str>,
    ) -> Result<Vec<EncryptedIndexInfo>, String> {
        match collection {
            Some(name) if self.in_txn().is_some() => {
                let context = self.context();
                let fields = self
                    .runner
                    .encrypted_indexes_in_txn_as(&context, name)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(fields.iter().map(|field| equality(name, field)).collect())
            }
            Some(name) => {
                self.allowed(NodePermission::EncryptedIndexList).await?;

                let col = self
                    .database
                    .get_collection(name)
                    .map_err(|e| format!("{}", e))?
                    .ok_or_else(|| format!("collection '{}' not found", name))?;

                Ok(col
                    .schema()
                    .encrypted_indexes
                    .iter()
                    .map(|ei| equality(name, &ei.field_name))
                    .collect())
            }
            None => {
                self.database
                    .check_node_access_as(&self.context(), NodePermission::EncryptedIndexListAll)
                    .await
                    .map_err(|e| format!("{}", e))?;

                let names = self
                    .database
                    .list_collections()
                    .map_err(|e| format!("{}", e))?;

                let mut result = Vec::new();
                for name in names {
                    if let Ok(Some(col)) = self.database.get_collection(&name) {
                        for ei in &col.schema().encrypted_indexes {
                            result.push(equality(&name, &ei.field_name));
                        }
                    }
                }
                Ok(result)
            }
        }
    }

    async fn delete_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<(), String> {
        if let Some(context) = self.in_txn() {
            return self
                .runner
                .delete_encrypted_index_in_txn_as(&context, collection, field_name)
                .await
                .map_err(|e| e.to_string());
        }
        self.allowed(NodePermission::EncryptedIndexDelete).await?;
        self.database
            .delete_encrypted_index(collection, field_name)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "../tests/unit/encrypted_index_adapter.rs"]
mod tests;
