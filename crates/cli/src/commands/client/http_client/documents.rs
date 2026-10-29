//! Document writes over Go's HTTP API.

use query::documents::{self, AddOptions, DocumentsResult, Filter};
use reqwest::{Method, Response};
use serde_json::{json, Value};
use urlencoding::encode;

use super::HttpClient;
use crate::error::{Error, Result};

impl HttpClient {
    /// Add `docs` to `collection`, all or none, and return their IDs in
    /// order. Go's `AddDocument` and `AddManyDocuments`.
    pub async fn add_docs(
        &self,
        txn: Option<&str>,
        collection: &str,
        docs: &[Value],
        options: &AddOptions,
    ) -> Result<Vec<String>> {
        let body = match docs {
            [doc] => doc.to_string(),
            many => Value::Array(many.to_vec()).to_string(),
        };
        let path = format!("collections/{}", encode(collection));
        let query = encryption(options);
        Ok(self
            .send(Method::POST, &path, &query, Some(body), txn)
            .await?
            .json()
            .await?)
    }

    /// Set the fields `patch` names on document `doc_id`. Go's
    /// `UpdateDocument`.
    pub async fn update_doc(
        &self,
        txn: Option<&str>,
        collection: &str,
        doc_id: &str,
        patch: &Value,
    ) -> Result<()> {
        let path = format!(
            "collections/{}/document/{}",
            encode(collection),
            encode(doc_id)
        );
        self.send(Method::PATCH, &path, &[], Some(patch.to_string()), txn)
            .await
            .map(drop)
    }

    /// Write `doc` as document `doc_id` when it exists, else add it, and
    /// return its ID. Go's `SaveDocument`: one transaction, the caller's
    /// when it names one.
    pub async fn save_doc(
        &self,
        txn: Option<&str>,
        collection: &str,
        doc_id: Option<&str>,
        doc: &Value,
        options: &AddOptions,
    ) -> Result<String> {
        let Some(doc_id) = doc_id else {
            let ids = self
                .add_docs(txn, collection, std::slice::from_ref(doc), options)
                .await?;
            return ids.into_iter().next().ok_or_else(not_found);
        };
        if let Some(txn) = txn {
            return self.save_in(txn, collection, doc_id, doc, options).await;
        }
        let txn = self.tx_begin(false).await?.id.to_string();
        match self.save_in(&txn, collection, doc_id, doc, options).await {
            Ok(id) => self.tx_commit(&txn).await.map(|()| id),
            Err(error) => {
                let _ = self.tx_discard(&txn).await;
                Err(error)
            }
        }
    }

    /// Delete document `doc_id`. Go's `DeleteDocument`.
    pub async fn delete_doc(
        &self,
        txn: Option<&str>,
        collection: &str,
        doc_id: &str,
    ) -> Result<()> {
        let path = format!(
            "collections/{}/document/{}",
            encode(collection),
            encode(doc_id)
        );
        self.send(Method::DELETE, &path, &[], None, txn)
            .await
            .map(drop)
    }

    /// Apply `updater`, a JSON object of fields, to every document `filter`
    /// matches. Go's `UpdateDocumentsWithFilter`.
    pub async fn update_with_filter(
        &self,
        txn: Option<&str>,
        collection: &str,
        filter: &Filter,
        updater: &str,
    ) -> Result<DocumentsResult> {
        let body = json!({"filter": filter_body(filter), "updater": updater}).to_string();
        let path = format!("collections/{}", encode(collection));
        Ok(self
            .send(Method::PATCH, &path, &[], Some(body), txn)
            .await?
            .json()
            .await?)
    }

    /// Delete every document `filter` matches. Go's
    /// `DeleteDocumentsWithFilter`.
    pub async fn delete_with_filter(
        &self,
        txn: Option<&str>,
        collection: &str,
        filter: &Filter,
    ) -> Result<DocumentsResult> {
        let body = json!({"filter": filter_body(filter)}).to_string();
        let path = format!("collections/{}", encode(collection));
        Ok(self
            .send(Method::DELETE, &path, &[], Some(body), txn)
            .await?
            .json()
            .await?)
    }

    /// Send `body` to `path` under the API root, in transaction `txn`, as
    /// Go's HTTP client does: the transaction travels in its header alone.
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<String>,
        txn: Option<&str>,
    ) -> Result<Response> {
        let url = format!("{}/api/v0/{path}", self.base_url);
        let mut request = self.add_auth_header(self.client.request(method, url).query(query));
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body);
        }
        if let Some(id) = txn {
            request = request.header("x-defradb-tx", id);
        }
        let response = request.send().await.map_err(Error::HttpRequest)?;
        match response.status().is_success() {
            true => Ok(response),
            false => Err(Self::extract_error(response).await),
        }
    }

    /// Go's `SaveDocument` over the wire: update the document when it
    /// exists, else add it, in transaction `txn`.
    async fn save_in(
        &self,
        txn: &str,
        collection: &str,
        doc_id: &str,
        doc: &Value,
        options: &AddOptions,
    ) -> Result<String> {
        let path = format!(
            "collections/{}/document/{}",
            encode(collection),
            encode(doc_id)
        );
        let shown = [("show_deleted", "true".to_owned())];
        match self.send(Method::GET, &path, &shown, None, Some(txn)).await {
            Ok(_) => {
                self.send(Method::PATCH, &path, &[], Some(doc.to_string()), Some(txn))
                    .await?;
                Ok(doc_id.to_owned())
            }
            Err(Error::Server(message)) if message.contains("document not found") => {
                let ids = self
                    .add_docs(Some(txn), collection, std::slice::from_ref(doc), options)
                    .await?;
                ids.into_iter().next().ok_or_else(not_found)
            }
            Err(error) => Err(error),
        }
    }
}

fn not_found() -> Error {
    Error::Server(documents::Error::DocumentNotFound.to_string())
}

/// Go's `encrypt` and `encryptFields` query parameters.
fn encryption(options: &AddOptions) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if options.encrypt_doc {
        query.push(("encrypt", "true".to_owned()));
    }
    if !options.encrypted_fields.is_empty() {
        query.push(("encryptFields", options.encrypted_fields.join(",")));
    }
    query
}

/// The filter as Go's handler takes it: an object or its source text.
fn filter_body(filter: &Filter) -> Value {
    match filter {
        Filter::Conditions(conditions) => Value::Object(conditions.clone()),
        Filter::Source(source) => Value::String(source.clone()),
        Filter::Unsupported(value) => value.clone(),
    }
}
