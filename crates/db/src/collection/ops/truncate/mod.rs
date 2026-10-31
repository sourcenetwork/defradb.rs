//! Truncating a collection, Go's `Collection.Truncate`: one entry point that
//! every edge calls.

mod all;
mod error;
mod filtered;

use std::sync::Arc;

use defra_core::RequestContext;
use query::documents;

pub use error::TruncateError;

use super::*;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Permanently remove the documents of collection `name` that `filter`
    /// matches, or every document without one, keeping the collection, as
    /// the caller `context` names. Go's `Collection.Truncate`.
    ///
    /// Truncate is not transactional: in a transaction an unfiltered
    /// truncate applies at once and a filtered one is refused.
    #[instrument(skip(self, context, filter), fields(collection = %name), name = "db.truncate_collection")]
    pub async fn truncate_collection_as(
        self: &Arc<Self>,
        context: &RequestContext,
        name: &str,
        filter: Option<&documents::Filter>,
    ) -> std::result::Result<(), TruncateError> {
        let collection = self
            .truncate_target(context, name, filter.is_some())
            .await?;
        let filter = filter.map(conditions).transpose()?;
        self.truncate(&collection, filter.as_ref()).await
    }

    /// [`Self::truncate_collection_as`] with the filter a GraphQL
    /// `truncate_<Collection>` mutation already parsed.
    pub(crate) async fn truncate_parsed_as(
        self: &Arc<Self>,
        context: &RequestContext,
        name: &str,
        filter: Option<&query::Filter>,
    ) -> std::result::Result<(), TruncateError> {
        let collection = self
            .truncate_target(context, name, filter.is_some())
            .await?;
        self.truncate(&collection, filter).await
    }

    /// The collection `name`, once the caller may truncate it and the
    /// truncate is one Go allows, checked in Go's order.
    async fn truncate_target(
        &self,
        context: &RequestContext,
        name: &str,
        filtered: bool,
    ) -> std::result::Result<Collection, TruncateError> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionTruncate)
            .await?;
        let collection = self
            .get_collection(name)?
            .ok_or_else(|| Error::CollectionNotFound(name.to_owned()))?;
        if filtered && collection.schema().is_branchable {
            return Err(TruncateError::FilteredBranchable);
        }
        if filtered && context.txn().is_some() {
            return Err(TruncateError::FilteredInTransaction);
        }
        Ok(collection)
    }

    /// Truncate `collection` under a truncate action, which `ListActions`
    /// shows while it runs and records if it fails.
    async fn truncate(
        self: &Arc<Self>,
        collection: &Collection,
        filter: Option<&query::Filter>,
    ) -> std::result::Result<(), TruncateError> {
        if let Some(filter) = filter {
            filter.validate_depth()?;
        }
        let collection_id = collection.collection_id();
        let action = self
            .register_action(collection_id, crate::Action::TRUNCATE)
            .await?;
        let truncated = match filter {
            Some(filter) => self.truncate_matching(collection, filter).await,
            None => self.truncate_all(collection).await,
        };
        let doc_count = match truncated {
            Ok(doc_count) => doc_count,
            Err(error) => {
                if let Err(action_error) = self.fail_action(action, &error.to_string()).await {
                    tracing::error!(
                        error = %action_error,
                        collection_id,
                        "Failed to record truncate action error"
                    );
                }
                return Err(error.into());
            }
        };
        self.complete_action(action).await?;
        tracing::info!(
            collection_id,
            short_id = collection.resolved_root_id(),
            doc_count,
            filtered = filter.is_some(),
            "Truncated collection"
        );
        Ok(())
    }
}

/// `filter` as conditions, refused with Go's words when it is not a filter.
fn conditions(filter: &documents::Filter) -> std::result::Result<query::Filter, TruncateError> {
    match filter {
        documents::Filter::Conditions(conditions) => {
            Ok(query::Filter::from_conditions(conditions.clone()))
        }
        documents::Filter::Source(source) if source.trim().is_empty() => {
            Err(TruncateError::EmptyFilter)
        }
        documents::Filter::Source(source) => match query::parse_filter_string(source)? {
            serde_json::Value::Object(conditions) => Ok(query::Filter::from_conditions(conditions)),
            _ => Err(TruncateError::UnsupportedFilterType),
        },
        documents::Filter::Unsupported(_) => Err(TruncateError::UnsupportedFilterType),
    }
}
