//! Secondary indexes as the caller `context` names.

use std::sync::Arc;

use acp::nac::NodePermission;
use db::IndexManager;
use defra_core::{ActionExecution, ActionStatus, RequestContext};
use schema::{IndexDescription, IndexedFieldDescription};
use serde::Serialize;

use crate::{Error, Node, Result};

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

    /// The indexes of collection `collection` with their build state, less
    /// the index the node adds for each primary relation on its own. Go's
    /// `Collection.GetIndexes`.
    pub async fn list_indexes_as(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<IndexListing>> {
        let database = Arc::clone(self.database());
        let context = self.caller(context);
        let name = collection.to_owned();
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::IndexList)
                .await?;
            let collection = database
                .get_collection(&name)?
                .ok_or_else(|| Error::CollectionMissing(name.clone()))?;
            let actions = database
                .list_index_actions_as(&context, collection.collection_id())
                .await?;
            Ok(collection
                .get_indexes()
                .iter()
                .filter(|index| !is_hidden_relation_index(&collection, index))
                .map(|index| IndexListing {
                    description: index.clone(),
                    collection_name: collection.name().to_string(),
                    execution: actions
                        .get(&index.id)
                        .cloned()
                        .unwrap_or_else(|| ActionExecution {
                            collection_id: collection.collection_id().to_string(),
                            subject: index.id.to_string(),
                            status: ActionStatus::COMPLETED,
                            ..Default::default()
                        }),
                })
                .collect())
        })
        .await
    }

    /// The actions running or ended with an error. Go's `ListActions`.
    pub async fn list_actions_as(&self, context: &RequestContext) -> Result<Vec<ActionExecution>> {
        let database = Arc::clone(self.database());
        let context = self.caller(context);
        self.on_pool(move || async move { Ok(database.list_actions_as(&context).await?) })
            .await
    }

    /// Delete index `name` of collection `collection`. Go's
    /// `Collection.DropIndex`.
    pub async fn delete_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        name: &str,
    ) -> Result<()> {
        let database = Arc::clone(self.database());
        let context = self.caller(context);
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::IndexDelete)
                .await?;
            Ok(database.drop_index(&collection, &name).await?)
        })
        .await
    }
}

/// Whether `index` is the one the node adds for a primary relation's
/// `_<name>ID` field, which Go does not list.
fn is_hidden_relation_index(collection: &db::Collection, index: &IndexDescription) -> bool {
    let [field] = index.fields.as_slice() else {
        return false;
    };
    if index.unique {
        return false;
    }
    let Some(relation) = field
        .name
        .strip_prefix('_')
        .and_then(|name| name.strip_suffix("ID"))
    else {
        return false;
    };
    index.name == format!("{}__{relation}ID_ASC", collection.name())
        && collection.schema().fields.iter().any(|candidate| {
            candidate.name == field.name
                && candidate.relation_name.is_some()
                && candidate.is_primary
                && matches!(
                    candidate.kind,
                    schema::FieldKind::Scalar(schema::ScalarKind::DocID)
                )
        })
}
