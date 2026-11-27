//! Document writes as the caller `context` names, in the transaction it
//! carries if it carries one. Each is a `query::documents` write run on the
//! node's request path.

use defra_core::RequestContext;
use query::documents::{self, AddOptions, DocumentsResult, Filter};
use query::QueryRequest;
use serde_json::Value;

use super::NodeCore;
use crate::ops::acting_identity;
use crate::Result;

impl NodeCore<'_> {
    /// Add `docs` to `collection`, all or none, and return their IDs in
    /// order. Go's `AddDocument` and `AddManyDocuments`.
    pub(crate) async fn add_docs(
        &self,
        context: &RequestContext,
        collection: &str,
        docs: &[Value],
        options: &AddOptions,
    ) -> Result<Vec<String>> {
        let identity = acting_identity(context)?;
        let run = |request| self.node.run_as_identity(context, identity, request);
        Ok(documents::add(run, collection, docs, options).await?)
    }

    /// Set the fields `patch` names on document `doc_id`. Go's
    /// `UpdateDocument`.
    pub(crate) async fn update_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        patch: &Value,
    ) -> Result<()> {
        let identity = acting_identity(context)?;
        let run = |mutation| {
            self.node
                .run_as_identity(context, identity, QueryRequest::new(mutation))
        };
        Ok(documents::update(run, collection, doc_id, patch).await?)
    }

    /// Refuse document `doc_id` when it has been deleted, else write `doc`
    /// as it when it exists, else add it, and return its ID. Go's
    /// `SaveDocument`: one transaction, the caller's when it names one.
    pub(crate) async fn save_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: Option<&str>,
        doc: &Value,
        options: &AddOptions,
    ) -> Result<String> {
        let identity = acting_identity(context)?;
        let save = |context: RequestContext| {
            let identity = identity.clone();
            async move {
                let run = |request| {
                    self.node
                        .run_as_identity(&context, identity.clone(), request)
                };
                documents::save(run, collection, doc_id, doc, options).await
            }
        };
        if doc_id.is_none() || context.txn().is_some() {
            return Ok(save(context.clone()).await?);
        }
        let txn = self.begin(context, false).await?.detach();
        let inner = context.clone().with_txn(Some(txn));
        match save(inner.clone()).await {
            Ok(id) => self.commit(&inner, txn).await.map(|()| id),
            Err(error) => {
                let _ = self.discard(&inner, txn).await;
                Err(error.into())
            }
        }
    }

    /// Save every document of `docs` in one transaction, each updated when
    /// its `_docID` names a stored document and added otherwise, and return
    /// their IDs in order. One request of typed mutations on the node's
    /// request path: in the transaction `context` carries, or as one
    /// engine-retried autocommit batch.
    pub(crate) async fn save_docs(
        &self,
        context: &RequestContext,
        collection: &str,
        docs: &[Value],
        options: &AddOptions,
    ) -> Result<Vec<String>> {
        let identity = acting_identity(context)?;
        let run = |request| self.node.run_as_identity(context, identity, request);
        Ok(documents::save_many(run, collection, docs, options).await?)
    }

    /// Delete document `doc_id`. Go's `DeleteDocument`.
    pub(crate) async fn delete_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
    ) -> Result<()> {
        let identity = acting_identity(context)?;
        let run = |mutation| {
            self.node
                .run_as_identity(context, identity, QueryRequest::new(mutation))
        };
        Ok(documents::delete(run, collection, doc_id).await?)
    }

    /// Apply `updater`, a JSON object of fields, to every document `filter`
    /// matches. Go's `UpdateDocumentsWithFilter`.
    pub(crate) async fn update_with_filter(
        &self,
        context: &RequestContext,
        collection: &str,
        filter: &Filter,
        updater: &str,
    ) -> Result<DocumentsResult> {
        let identity = acting_identity(context)?;
        let run = |request| self.node.run_as_identity(context, identity, request);
        Ok(documents::update_with_filter(run, collection, filter, updater).await?)
    }

    /// Delete every document `filter` matches. Go's
    /// `DeleteDocumentsWithFilter`.
    pub(crate) async fn delete_with_filter(
        &self,
        context: &RequestContext,
        collection: &str,
        filter: &Filter,
    ) -> Result<DocumentsResult> {
        let identity = acting_identity(context)?;
        let run = |request| self.node.run_as_identity(context, identity, request);
        Ok(documents::delete_with_filter(run, collection, filter).await?)
    }
}
