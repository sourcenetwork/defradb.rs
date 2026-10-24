//! Adapter to bridge database index operations to HTTP's IndexOperations trait.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use db::IndexManager;
use defra_http::router::{IndexFieldInfo, IndexInfo, IndexOperations};
use schema::IndexedFieldDescription;
use storage::corekv::Store;

/// Adapter that implements IndexOperations using the database.
pub struct IndexAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> IndexAdapter<S> {
    pub fn new(database: Arc<db::DB<S>>) -> Self {
        Self {
            database,
            context: None,
        }
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }

    pub fn new_arc(database: Arc<db::DB<S>>) -> Arc<dyn IndexOperations> {
        Arc::new(Self::new(database))
    }
}

#[async_trait]
impl<S: Store + 'static> IndexOperations for IndexAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn IndexOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            context: Some(context.clone()),
        }))
    }

    async fn create_index(
        &self,
        collection: &str,
        fields: Vec<String>,
        name: Option<&str>,
        unique: bool,
        vector: Option<schema::VectorIndexDescription>,
    ) -> Result<IndexInfo, String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::IndexCreate)
            .await
            .map_err(|e| format!("{}", e))?;

        let indexed_fields: Vec<IndexedFieldDescription> = fields
            .iter()
            .map(|f| IndexedFieldDescription {
                name: f.clone(),
                descending: false,
            })
            .collect();
        let kind = IndexManager::requested_kind(&indexed_fields, unique, vector)
            .map_err(|e| format!("{}", e))?;
        let index_desc = self
            .database
            .create_index(collection, name, indexed_fields, kind)
            .await
            .map_err(|e| format!("{}", e))?;
        let collection_id = self
            .database
            .require_collection(collection)
            .map_err(|e| format!("{}", e))?
            .collection_id()
            .to_string();

        Ok(IndexInfo {
            kind: index_desc.kind,
            id: index_desc.id,
            name: index_desc.name,
            collection: collection.to_string(),
            collection_id,
            fields: index_desc
                .fields
                .into_iter()
                .map(|f| IndexFieldInfo {
                    name: f.name,
                    direction: Some(if f.descending {
                        "DESC".to_string()
                    } else {
                        "ASC".to_string()
                    }),
                })
                .collect(),
            unique: index_desc.unique,
        })
    }

    async fn list_indexes(&self, collection: Option<&str>) -> Result<Vec<IndexInfo>, String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::IndexList)
            .await
            .map_err(|e| format!("{}", e))?;

        let collections = if let Some(name) = collection {
            let col = self
                .database
                .require_collection(name)
                .map_err(|e| format!("{}", e))?;
            vec![col]
        } else {
            let names = self
                .database
                .list_collections()
                .map_err(|e| format!("{}", e))?;
            let mut cols = Vec::new();
            for name in &names {
                if let Some(col) = self
                    .database
                    .get_collection(name)
                    .map_err(|e| format!("{}", e))?
                {
                    cols.push(col);
                }
            }
            cols
        };

        let mut result = Vec::new();
        for col in &collections {
            for idx in col.get_indexes() {
                result.push(IndexInfo {
                    kind: idx.kind,
                    id: idx.id,
                    name: idx.name.clone(),
                    collection: col.name().to_string(),
                    collection_id: col.collection_id().to_string(),
                    fields: idx
                        .fields
                        .iter()
                        .map(|f| IndexFieldInfo {
                            name: f.name.clone(),
                            direction: Some(if f.descending {
                                "DESC".to_string()
                            } else {
                                "ASC".to_string()
                            }),
                        })
                        .collect(),
                    unique: idx.unique,
                });
            }
        }

        Ok(result)
    }

    async fn delete_index(&self, collection: &str, name: &str) -> Result<(), String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::IndexDelete)
            .await
            .map_err(|e| format!("{}", e))?;

        self.database
            .drop_index(collection, name)
            .await
            .map_err(|e| format!("{}", e))
    }
}

#[cfg(test)]
#[path = "../tests/unit/index_adapter.rs"]
mod tests;
