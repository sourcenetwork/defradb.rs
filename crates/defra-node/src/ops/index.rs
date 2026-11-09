//! Secondary indexes as the caller `context` names.

use std::sync::Arc;

use acp::nac::NodePermission;
use db::IndexManager;
use defra_core::{ActionExecution, ActionStatus, RequestContext};
use schema::{IndexDescription, IndexedFieldDescription};
use serde::Serialize;

use crate::ops::request::transaction_error;
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
        self.new_index_as(context, collection, name, fields, unique, true)
            .await
    }

    /// [`Self::create_index_as`], returning once the index is defined: it is
    /// listed as building until its backfill completes. Go's
    /// `Collection.NewIndex`.
    pub async fn start_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
    ) -> Result<IndexDescription> {
        self.new_index_as(context, collection, name, fields, unique, false)
            .await
    }

    async fn new_index_as(
        &self,
        context: &RequestContext,
        collection: &str,
        name: Option<&str>,
        fields: Vec<IndexedFieldDescription>,
        unique: bool,
        wait_for_build: bool,
    ) -> Result<IndexDescription> {
        let caller = self.caller(context);
        if caller.txn().is_some() {
            self.check_access(&caller, NodePermission::CollectionGet)
                .await?;
            self.check_access(&caller, NodePermission::IndexCreate)
                .await?;
            let kind =
                IndexManager::requested_kind(&fields, unique, None).map_err(db::Error::from)?;
            return self
                .runner()
                .create_index_in_txn_as(&caller, collection, name, fields, kind)
                .await
                .map_err(transaction_error);
        }
        let database = Arc::clone(self.database());
        let context = caller;
        let (collection, name) = (collection.to_owned(), name.map(str::to_owned));
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            database
                .check_node_access_as(&context, NodePermission::IndexCreate)
                .await?;
            let kind =
                IndexManager::requested_kind(&fields, unique, None).map_err(db::Error::from)?;
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
    pub async fn list_indexes_as(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<IndexListing>> {
        let caller = self.caller(context);
        if caller.txn().is_some() {
            self.check_access(&caller, NodePermission::CollectionGet)
                .await?;
            self.check_access(&caller, NodePermission::IndexList)
                .await?;
            let (version, actions) = self
                .runner()
                .indexes_in_txn_as(&caller, collection)
                .await
                .map_err(transaction_error)?;
            return Ok(listings(&version, &actions));
        }
        let database = Arc::clone(self.database());
        let context = caller;
        let name = collection.to_owned();
        self.on_pool(move || async move {
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
        let caller = self.caller(context);
        if caller.txn().is_some() {
            self.check_access(&caller, NodePermission::CollectionGet)
                .await?;
            self.check_access(&caller, NodePermission::IndexDelete)
                .await?;
            return self
                .runner()
                .drop_index_in_txn_as(&caller, collection, name)
                .await
                .map_err(transaction_error);
        }
        let database = Arc::clone(self.database());
        let context = caller;
        let (collection, name) = (collection.to_owned(), name.to_owned());
        self.on_pool(move || async move {
            database
                .check_node_access_as(&context, NodePermission::CollectionGet)
                .await?;
            database
                .check_node_access_as(&context, NodePermission::IndexDelete)
                .await?;
            database
                .drop_index(&collection, &name)
                .await
                .map_err(|error| match error {
                    db::Error::IndexNotFound { index, .. } => Error::IndexNameMissing(index),
                    other => Error::from(other),
                })
        })
        .await
    }
}

/// The indexes of `version` with their build state, less the index the node
/// adds for each primary relation on its own. An index without an action
/// record is complete.
fn listings(
    version: &schema::CollectionVersion,
    actions: &rapidhash::RapidHashMap<u32, ActionExecution>,
) -> Vec<IndexListing> {
    version
        .indexes
        .iter()
        .filter(|index| !is_hidden_relation_index(version, index))
        .map(|index| IndexListing {
            description: index.clone(),
            collection_name: version.name.clone(),
            execution: actions
                .get(&index.id)
                .cloned()
                .unwrap_or_else(|| ActionExecution {
                    collection_id: version.collection_id.clone(),
                    subject: index.id.to_string(),
                    status: ActionStatus::COMPLETED,
                    ..Default::default()
                }),
        })
        .collect()
}

/// Whether `index` is the one the node adds for a primary relation's
/// `_<name>ID` field, which Go does not list.
fn is_hidden_relation_index(version: &schema::CollectionVersion, index: &IndexDescription) -> bool {
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
    index.name == format!("{}__{relation}ID_ASC", version.name)
        && version.fields.iter().any(|candidate| {
            candidate.name == field.name
                && candidate.relation_name.is_some()
                && candidate.is_primary
                && matches!(
                    candidate.kind,
                    schema::FieldKind::Scalar(schema::ScalarKind::DocID)
                )
        })
}
