//! Secondary indexes as the caller `context` names.

use std::sync::Arc;

use acp::nac::NodePermission;
use db::IndexManager;
use defra_core::RequestContext;
use schema::{IndexDescription, IndexedFieldDescription};

use crate::{Node, Result};

impl Node {
    /// Create an ordered index named `name` (generated when `None`) over
    /// `fields` of collection `collection`, `unique` or not, and index the
    /// documents it already holds. Go's `Collection.CreateIndex`.
    pub async fn create_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
    ) -> Result<IndexDescription> {
        let database = Arc::clone(self.database());
        let context = self.caller(context);
        let (collection, name) = (collection.to_owned(), name.map(str::to_owned));
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::IndexCreate)
                .await?;
            let kind =
                IndexManager::requested_kind(&fields, unique, None).map_err(db::Error::from)?;
            Ok(database
                .create_index(&collection, name.as_deref(), fields, kind)
                .await?)
        })
        .await
    }
}
