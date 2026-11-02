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

    /// Write `doc` as document `doc_id` when it exists, else add it, and
    /// return its ID. Go's `SaveDocument`.
    pub async fn save_doc(
        &self,
        context: &RequestContext,
        collection: &str,
        doc_id: Option<&str>,
        doc: &Value,
        options: &AddOptions,
    ) -> Result<String> {
        let identity = acting_identity(context)?;
        let run = |mutation| self.run_as_identity(context, identity, QueryRequest::new(mutation));
        Ok(documents::save(run, collection, doc_id, doc, options).await?)
    }

    /// Save every document of `docs` in one transaction, each updated when
    /// its `_docID` names a stored document and added otherwise, and return
    /// their IDs in order. One request of typed mutations on the node's
    /// request path: in the transaction `context` carries, or as one
    /// engine-retried autocommit batch.
    pub async fn save_docs(
        &self,
        context: &RequestContext,
        collection: &str,
        docs: &[Value],
        options: &AddOptions,
    ) -> Result<Vec<String>> {
        let identity = acting_identity(context)?;
        let run = |request| self.run_as_identity(context, identity, request);
        Ok(documents::save_many(run, collection, docs, options).await?)
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
