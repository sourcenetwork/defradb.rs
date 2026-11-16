//! Read checks for chunk documents outside a query plan. A chunk is readable
//! only while its parent is live, lists it, and is readable.

use std::sync::Arc;

use acp::Identity;
use identity::Did;
use rapidhash::{RapidHashMap, RapidHashSet};
use schema::{ChunkedField, CollectionVersion};

use super::{DocFetcher, QueryRunner};
use crate::error::Result;
use crate::plan::{chunk_fields, get_projected, parent_fields, parent_hashes, ChunkFields};
use crate::txn::TransactionRegistry;

type Parent = Option<(Arc<CollectionVersion>, ChunkedField)>;

/// The parent decisions one request has made, so chunks of one parent share one.
#[derive(Default)]
pub(crate) struct ChunkReads {
    parents: RapidHashMap<String, Parent>,
    decided: RapidHashMap<(String, String), Option<RapidHashSet<String>>>,
}

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    pub(crate) async fn chunk_readable(
        &self,
        reads: &mut ChunkReads,
        fetcher: &dyn DocFetcher,
        identity: &Identity,
        caller: Option<&Did>,
        chunks_col: &CollectionVersion,
        doc_id: &str,
    ) -> Result<bool> {
        let Some((parent, chunked)) = self.chunk_parent(reads, chunks_col).await else {
            return Ok(false);
        };
        let Some(chunk) = get_projected(fetcher, &chunks_col.name, doc_id, &chunk_fields())
            .await?
            .as_ref()
            .and_then(ChunkFields::from_document)
        else {
            return Ok(false);
        };
        let Some(hash) = chunk.verified_hash() else {
            return Ok(false);
        };
        let key = (chunks_col.name.clone(), chunk.parent_id.clone());
        if !reads.decided.contains_key(&key) {
            let decision = self
                .decide_parent(
                    fetcher,
                    identity,
                    caller,
                    &parent,
                    &chunked,
                    &chunk.parent_id,
                )
                .await?;
            reads.decided.insert(key.clone(), decision);
        }
        Ok(reads
            .decided
            .get(&key)
            .and_then(Option::as_ref)
            .is_some_and(|hashes| hashes.contains(hash)))
    }

    async fn chunk_parent(&self, reads: &mut ChunkReads, chunks_col: &CollectionVersion) -> Parent {
        if let Some(parent) = reads.parents.get(&chunks_col.name) {
            return parent.clone();
        }
        let parent = match &chunks_col.chunk_of {
            None => None,
            Some(source) => match self.get_collection(&source.collection).await {
                Ok(parent) => parent
                    .chunked_field(&source.field_name)
                    .cloned()
                    .map(|chunked| (Arc::clone(&parent), chunked)),
                Err(error) => {
                    tracing::warn!(chunks = %chunks_col.name, %error, "chunk parent lookup failed");
                    None
                }
            },
        };
        reads
            .parents
            .insert(chunks_col.name.clone(), parent.clone());
        parent
    }

    /// The chunk hashes `parent_id`'s current text uses, or `None` when it
    /// isn't live or the reader may not read it.
    async fn decide_parent(
        &self,
        fetcher: &dyn DocFetcher,
        identity: &Identity,
        caller: Option<&Did>,
        parent: &CollectionVersion,
        chunked: &ChunkedField,
        parent_id: &str,
    ) -> Result<Option<RapidHashSet<String>>> {
        let Some(hashes) = get_projected(fetcher, &parent.name, parent_id, &parent_fields(chunked))
            .await?
            .and_then(|doc| parent_hashes(&doc, chunked))
        else {
            return Ok(None);
        };
        if let Some(policy) = &parent.policy {
            let allowed = crate::txn::check_doc_read_access_as(
                &self.context(),
                self.acp.as_ref(),
                self.acp_registration_projection.as_deref(),
                identity,
                &policy.id,
                &policy.resource_name,
                &parent.collection_id,
                parent.is_branchable,
                parent_id,
                self.retry_inputs.as_ref(),
            )
            .await?;
            if !allowed {
                return Ok(None);
            }
        }
        if !self.app_may_read(caller, parent, parent_id).await? {
            return Ok(None);
        }
        Ok(Some(hashes))
    }
}
