//! Document reads scoped to an open transaction.

use super::*;
use document::Document;
use query::txn::TxnTurn;

impl<S: Store + 'static> DbTransactionRegistry<S> {
    /// [`Self::get_all_docs_as`] in [`RequestContext::ambient`].
    pub async fn get_all_docs(&self, txn_id: &str, collection_name: &str) -> Result<Vec<Document>> {
        self.get_all_docs_as(&RequestContext::ambient(), txn_id, collection_name)
            .await
    }

    /// Get all documents from a collection within a transaction.
    pub async fn get_all_docs_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection_name: &str,
    ) -> Result<Vec<Document>> {
        let collection_name = collection_name.to_owned();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let context = owned
                    .context()
                    .map_err(|error| Error::Other(error.to_string()))?;
                context
                    .doc_fetcher()
                    .get_all(&collection_name)
                    .await
                    .map_err(Error::Query)
            })
        })
        .await
    }

    /// [`Self::get_docs_by_ids_as`] in [`RequestContext::ambient`].
    pub async fn get_docs_by_ids(
        &self,
        txn_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> Result<Vec<Document>> {
        self.get_docs_by_ids_as(&RequestContext::ambient(), txn_id, collection_name, doc_ids)
            .await
    }

    /// Get documents by IDs from a collection within a transaction.
    ///
    /// Note: This convenience method returns only the found documents.
    /// For information about missing IDs, use the DocFetcher's get_by_ids directly.
    pub async fn get_docs_by_ids_as(
        &self,
        context: &RequestContext,
        txn_id: &str,
        collection_name: &str,
        doc_ids: &[String],
    ) -> Result<Vec<Document>> {
        let collection_name = collection_name.to_owned();
        let doc_ids = doc_ids.to_vec();
        self.in_txn(context, txn_id, move |owned| {
            Box::pin(async move {
                let context = owned
                    .context()
                    .map_err(|error| Error::Other(error.to_string()))?;
                context
                    .doc_fetcher()
                    .get_by_ids(&collection_name, &doc_ids)
                    .await
                    .map(|result| result.into_docs())
                    .map_err(Error::Query)
            })
        })
        .await
    }
}
