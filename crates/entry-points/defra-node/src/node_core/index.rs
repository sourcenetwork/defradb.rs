//! Secondary indexes as the caller `context` names.

use std::sync::Arc;

use acp::nac::NodePermission;
use db::IndexManager;
use defra_core::{ActionExecution, RequestContext};
use schema::{
    FullTextIndexDescription, IndexDescription, IndexedFieldDescription, VectorIndexDescription,
};
use serde::Serialize;

use super::NodeCore;
use crate::ops::transaction_error;
use crate::{Error, Result};

/// An index and the state of the action that builds it, as Go's
/// `Collection.GetIndexes` returns them.
#[derive(Debug, Clone, Serialize)]
pub struct IndexListing {
    #[serde(rename = "Description")]
    pub description: IndexDescription,
    #[serde(rename = "CollectionName")]
    pub collection_name: String,
    #[serde(rename = "Execution")]
    pub execution: ActionExecution,
}

impl NodeCore<'_> {
    /// Create an index named `name` (generated when `None`) over `fields`
    /// of collection `collection`, `unique` or not, and index the documents
    /// it already holds: a vector index when `vector` describes one, an
    /// ordered one otherwise. Go's `Collection.CreateIndex`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
        vector: Option<VectorIndexDescription>,
        fulltext: Option<FullTextIndexDescription>,
    ) -> Result<IndexDescription> {
        self.new_index(
            context, collection, name, fields, unique, vector, fulltext, true,
        )
        .await
    }

    /// [`Self::create_index`], returning once the index is defined: it is
    /// listed as building until its backfill completes. Go's
    /// `Collection.NewIndex`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
        vector: Option<VectorIndexDescription>,
        fulltext: Option<FullTextIndexDescription>,
    ) -> Result<IndexDescription> {
        self.new_index(
            context, collection, name, fields, unique, vector, fulltext, false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn new_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
        vector: Option<VectorIndexDescription>,
        fulltext: Option<FullTextIndexDescription>,
        wait_for_build: bool,
    ) -> Result<IndexDescription> {
        let caller = self.node.admitted_caller(context)?;
        if caller.txn().is_some() {
            let kind = IndexManager::requested_kind(&fields, unique, vector, fulltext)
                .map_err(db::Error::from)?;
            return self
                .node
                .runner()
                .create_index_in_txn_as(&caller, collection, name, fields, kind)
                .await
                .map_err(transaction_error);
        }
        let database = Arc::clone(self.node.database());
        let context = caller;
        let (collection, name) = (collection.to_owned(), name.map(str::to_owned));
        self.node
            .on_pool(move || async move {
                database
                    .check_node_access_as(&context, NodePermission::CollectionGet)
                    .await?;
                database
                    .check_node_access_as(&context, NodePermission::IndexCreate)
                    .await?;
                let kind = IndexManager::requested_kind(&fields, unique, vector, fulltext)
                    .map_err(db::Error::from)?;
                Ok(match wait_for_build {
                    true => {
                        database
                            .create_index(&collection, name.as_deref(), fields, kind)
                            .await?
                    }
                    false => {
                        database
                            .start_index(&collection, name.as_deref(), fields, kind)
                            .await?
                    }
                })
            })
            .await
    }

    /// The indexes of collection `collection` with their build state, less
    /// the index the node adds for each primary relation on its own. Go's
    /// `Collection.GetIndexes`.
    pub(crate) async fn list_indexes(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<IndexListing>> {
        let caller = self.node.admitted_caller(context)?;
        if caller.txn().is_some() {
            let (version, actions) = self
                .node
                .runner()
                .indexes_in_txn_as(&caller, collection)
                .await
                .map_err(transaction_error)?;
            return Ok(listings(&version, &actions));
        }
        let database = Arc::clone(self.node.database());
        let context = caller;
        let name = collection.to_owned();
        self.node
            .on_pool(move || async move {
                database
                    .check_node_access_as(&context, NodePermission::CollectionGet)
                    .await?;
                database
                    .check_node_access_as(&context, NodePermission::IndexList)
                    .await?;
                let collection = database
                    .get_collection(&name)?
                    .ok_or_else(|| Error::CollectionMissing(name.clone()))?;
                let actions = database
                    .list_index_actions_as(&context, collection.collection_id())
                    .await?;
                Ok(listings(collection.schema(), &actions))
            })
            .await
    }

    /// The actions running or ended with an error. Go's `ListActions`.
    pub(crate) async fn list_actions(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<ActionExecution>> {
        let database = Arc::clone(self.node.database());
        let context = self.node.admitted_caller(context)?;
        self.node
            .on_pool(move || async move { Ok(database.list_actions_as(&context).await?) })
            .await
    }

    /// Delete index `name` of collection `collection`. Go's
    /// `Collection.DropIndex`.
    pub(crate) async fn delete_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: &str,
    ) -> Result<()> {
        let caller = self.node.admitted_caller(context)?;
        if caller.txn().is_some() {
            return self
                .node
                .runner()
                .drop_index_in_txn_as(&caller, collection, name)
                .await
                .map_err(transaction_error);
        }
        let database = Arc::clone(self.node.database());
        let context = caller;
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.node
            .on_pool(move || async move {
                database
                    .check_node_access_as(&context, NodePermission::CollectionGet)
                    .await?;
                database
                    .check_node_access_as(&context, NodePermission::IndexDelete)
                    .await?;
                Ok(database.drop_index(&collection, &name).await?)
            })
            .await
    }
}

/// [`db::index_listing`], as the node returns it.
fn listings(
    version: &schema::CollectionVersion,
    actions: &rapidhash::RapidHashMap<u32, ActionExecution>,
) -> Vec<IndexListing> {
    db::index_listing(version, actions)
        .into_iter()
        .map(|(index, execution)| IndexListing {
            description: index,
            collection_name: version.name.clone(),
            execution,
        })
        .collect()
}
