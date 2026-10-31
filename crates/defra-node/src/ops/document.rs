//! Document writes as the caller `context` names, in the transaction it
//! carries if it carries one. Each is a `query::documents` write run on the
//! node's request path.

use defra_core::RequestContext;
use query::documents::{self, AddOptions, DocumentsResult, Filter};
use query::QueryRequest;
use serde_json::Value;

use crate::ops::request::acting_identity;
use crate::{Node, Result};

impl Node {
    /// Add `docs` to `collection`, all or none, and return their IDs in
    /// order. Go's `AddDocument` and `AddManyDocuments`.
    pub async fn add_docs(
        &self,
        context: &RequestContext,
        collection: &str,
        docs: &[Value],
        options: &AddOptions,
    ) -> Result<Vec<String>> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::add(run, collection, docs, options).await?)
    }

    /// Set the fields `patch` names on document `doc_id`. Go's
    /// `UpdateDocument`.
    pub async fn update_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
        patch: &Value,
    ) -> Result<()> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::update(run, collection, doc_id, patch).await?)
    }

    /// Refuse document `doc_id` when it has been deleted, else write `doc`
    /// as it when it exists, else add it, and return its ID. Go's
    /// `SaveDocument`: one transaction, the caller's when it names one.
    pub async fn save_doc(
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
                    self.run_as_identity(&context, identity.clone(), QueryRequest::new(request))
                };
                documents::save(run, collection, doc_id, doc, options).await
            }
        };
        if doc_id.is_none() || context.txn().is_some() {
            return Ok(save(context.clone()).await?);
        }
        let txn = self.txn_begin(context, false).await?;
        let inner = context.clone().with_txn(Some(txn));
        match save(inner.clone()).await {
            Ok(id) => self.txn_commit(&inner, txn).await.map(|()| id),
            Err(error) => {
                let _ = self.txn_discard(&inner, txn).await;
                Err(error.into())
            }
        }
    }

    /// Delete document `doc_id`. Go's `DeleteDocument`.
    pub async fn delete_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: &str,
    ) -> Result<()> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::delete(run, collection, doc_id).await?)
    }

    /// Apply `updater`, a JSON object of fields, to every document `filter`
    /// matches. Go's `UpdateDocumentsWithFilter`.
    pub async fn update_with_filter(
        &self,
        context: &RequestContext,
        collection: &str,
        filter: &Filter,
        updater: &str,
    ) -> Result<DocumentsResult> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::update_with_filter(run, collection, filter, updater).await?)
    }

    /// Delete every document `filter` matches. Go's
    /// `DeleteDocumentsWithFilter`.
    pub async fn delete_with_filter(
        &self,
        context: &RequestContext,
        collection: &str,
        filter: &Filter,
    ) -> Result<DocumentsResult> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::delete_with_filter(run, collection, filter).await?)
    }
}
