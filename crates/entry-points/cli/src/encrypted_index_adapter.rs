//! HTTP's EncryptedIndexOperations over the core's encrypted index
//! operations. A request in a transaction runs in it.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{EncryptedIndexInfo, EncryptedIndexOperations};
use storage::corekv::Store;

/// The core's encrypted index operations, serving one request once bound
/// to it.
pub struct EncryptedIndexAdapter<S>
where
    S: Store + 'static,
{
    registry: Arc<db::DbTransactionRegistry<S>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S> EncryptedIndexAdapter<S>
where
    S: Store + 'static,
{
    pub fn new_arc(
        registry: Arc<db::DbTransactionRegistry<S>>,
    ) -> Arc<dyn EncryptedIndexOperations> {
        Arc::new(Self {
            registry,
            context: None,
        })
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }
}

fn equality(collection: &str, field_name: &str) -> EncryptedIndexInfo {
    EncryptedIndexInfo {
        collection: collection.to_string(),
        field_name: field_name.to_string(),
        index_type: "equality".to_string(),
    }
}

fn text(error: db::EncryptedIndexError) -> String {
    error.to_string()
}

#[async_trait]
impl<S> EncryptedIndexOperations for EncryptedIndexAdapter<S>
where
    S: Store + 'static,
{
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn EncryptedIndexOperations>> {
        Some(Arc::new(Self {
            registry: Arc::clone(&self.registry),
            context: Some(context.clone()),
        }))
    }

    async fn add_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexInfo, String> {
        self.registry
            .add_encrypted_index_as(&self.context(), collection, field_name)
            .await
            .map_err(text)?;
        Ok(equality(collection, field_name))
    }

    async fn list_encrypted_indexes(
        &self,
        collection: Option<&str>,
    ) -> Result<Vec<EncryptedIndexInfo>, String> {
        let context = self.context();
        match collection {
            Some(name) => {
                let fields = self
                    .registry
                    .encrypted_indexes_as(&context, name)
                    .await
                    .map_err(text)?;
                Ok(fields.iter().map(|field| equality(name, field)).collect())
            }
            None => {
                let all = self
                    .registry
                    .all_encrypted_indexes_as(&context)
                    .await
                    .map_err(text)?;
                Ok(all
                    .iter()
                    .flat_map(|(name, fields)| {
                        fields.iter().map(move |field| equality(name, field))
                    })
                    .collect())
            }
        }
    }

    async fn delete_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<(), String> {
        self.registry
            .delete_encrypted_index_as(&self.context(), collection, field_name)
            .await
            .map_err(text)
    }
}
