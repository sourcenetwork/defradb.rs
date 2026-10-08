use async_trait::async_trait;
use document::Document;

use crate::error::{QueryError, Result};
use crate::fetcher::{DocFetcher, FetchByIdsResult};

pub(super) struct PreparationFetcher<'a> {
    pub base: &'a dyn DocFetcher,
    pub documents: &'a [(String, String, Option<Document>)],
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocFetcher for PreparationFetcher<'_> {
    async fn get_all(&self, collection: &str) -> Result<Vec<Document>> {
        let mut docs = self.base.get_all(collection).await?;
        for (name, id, projected) in self.documents {
            if name != collection {
                continue;
            }
            docs.retain(|doc| doc.id().is_none_or(|doc_id| doc_id.to_string() != *id));
            if let Some(projected) = projected {
                docs.push(projected.clone());
            }
        }
        Ok(docs)
    }

    async fn get_by_ids(&self, collection: &str, ids: &[String]) -> Result<FetchByIdsResult> {
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for id in ids {
            if let Some((_, _, doc)) = self
                .documents
                .iter()
                .rev()
                .find(|(name, projected_id, _)| name == collection && projected_id == id)
            {
                if let Some(doc) = doc {
                    found.push(doc.clone());
                } else {
                    missing.push(id.clone());
                }
            } else {
                let result = self
                    .base
                    .get_by_ids(collection, std::slice::from_ref(id))
                    .await?;
                missing.extend_from_slice(result.missing_ids());
                found.extend(result.into_docs());
            }
        }
        Ok(FetchByIdsResult::partial(found, missing))
    }

    async fn stream_by_doc_short_ids(
        &self,
        _: &str,
        _: &[u64],
        _: bool,
    ) -> Result<Box<dyn crate::doc_stream::DocStream>> {
        Err(QueryError::execution(
            "key preparation does not stream documents",
        ))
    }

    async fn stream_all_with_deleted(
        &self,
        _: &str,
        _: bool,
    ) -> Result<Box<dyn crate::doc_stream::DocStream>> {
        Err(QueryError::execution(
            "key preparation does not stream documents",
        ))
    }

    async fn get_by_field_value(&self, _: &str, _: &str, _: &str) -> Result<Vec<Document>> {
        Err(QueryError::execution(
            "key preparation does not resolve relations",
        ))
    }
}
