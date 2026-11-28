//! The chunk embeddings a create will ask for, known before its transaction.

use document::Document;
use query::error::{QueryError, Result};
use query::prepared::PreparedInputRequest;
use schema::CollectionVersion;
use storage::corekv::Store;

use crate::collection::Collection;
use crate::database::DB;
use crate::search::embedding::prepared_embedding_request;

use super::{chunk_document, plan_chunks};

/// The embedding requests the chunks of the new documents `docs` will make.
pub(crate) fn embedding_requests<S>(
    db: &DB<S>,
    parent: &CollectionVersion,
    docs: &[Document],
) -> Result<Vec<PreparedInputRequest>>
where
    S: Store + 'static,
{
    let mut embedded: Vec<(String, Collection)> = Vec::new();
    for chunked in &parent.chunked_fields {
        let name = chunked.collection(&parent.name);
        let collection = db
            .get_collection(&name)
            .map_err(|e| QueryError::execution(e.to_string()))?;
        if let Some(collection) =
            collection.filter(|chunks| !chunks.schema().vector_embeddings.is_empty())
        {
            embedded.push((name, collection));
        }
    }
    if embedded.is_empty() {
        return Ok(Vec::new());
    }
    let config = db.options().embedding_config();
    let mut requests = Vec::new();
    for doc in docs {
        for plan in plan_chunks(parent, doc, None, None)? {
            let Some((_, chunks)) = embedded
                .iter()
                .find(|(name, _)| *name == plan.chunk_collection)
            else {
                continue;
            };
            for (text, hash) in plan.chunks {
                // The request names only the text and the model, so the parent's ID is not needed.
                let chunk = chunk_document(chunks, "", text, &hash)?;
                // A missing model or URL is reported by the attempt itself.
                requests.extend(
                    chunks
                        .schema()
                        .vector_embeddings
                        .iter()
                        .filter_map(|embedding| {
                            prepared_embedding_request(embedding, &chunk, &config).ok()
                        }),
                );
            }
        }
    }
    Ok(requests)
}
