//! A single write to a collection with `@chunk` fields runs as a batch of one,
//! so chunk writing lives in one place, the batch path.

use std::any::Any;
use std::sync::Arc;

use defra_core::thread_bounds::MaybeBoxFuture;
use defra_core::RequestContext;
use document::{DocID, Document};
use query::error::{QueryError, Result};
use query::mutator::DocMutator;
use query::prepared::PreparedInputMemo;
use rapidhash::RapidHashSet;
use storage::corekv::{RetryAttemptValue, Store, TransactionConflict};

use super::batch::{AttemptWork, BatchRetryAttempt};
use super::AutoCommitMutator;

/// Which path a single write takes.
pub(super) enum Route {
    /// The normal single-write path.
    Direct,
    /// A batch of one, for collections with `@chunk` fields.
    Batch,
}

#[derive(Clone)]
pub(super) struct CreateWork {
    pub(super) context: RequestContext,
    pub(super) collection_name: String,
    pub(super) docs: Vec<Document>,
}

#[derive(Clone)]
pub(super) struct UpdateWork {
    pub(super) context: RequestContext,
    pub(super) collection_name: String,
    pub(super) doc: Document,
    pub(super) modified_fields: RapidHashSet<String>,
    /// The document the caller selected, which must still be current.
    pub(super) expected: Option<Document>,
    /// The fields of `expected` that must match. `None` means all of them.
    pub(super) expected_fields: Option<RapidHashSet<String>>,
}

/// Whether `current` is still the document the caller selected.
fn unchanged(
    expected: &Document,
    fields: Option<&RapidHashSet<String>>,
    current: &Document,
) -> bool {
    let values_match = match fields {
        Some(fields) => fields
            .iter()
            .all(|field| expected.get(field) == current.get(field)),
        None => {
            expected.values().len() == current.values().len()
                && expected
                    .values()
                    .iter()
                    .all(|(name, value)| current.get(name) == Some(value.value()))
        }
    };
    values_match && expected.id() == current.id() && expected.is_deleted() == current.is_deleted()
}

#[derive(Clone)]
pub(super) struct DeleteWork {
    pub(super) context: RequestContext,
    pub(super) collection_name: String,
    pub(super) doc_ids: Vec<DocID>,
}

fn boxed<T>(value: T) -> RetryAttemptValue
where
    T: Any + Send,
{
    Box::new(value)
}

impl AttemptWork for CreateWork {
    fn run(
        &self,
        mutator: Arc<dyn DocMutator>,
        _fetcher: Arc<dyn query::fetcher::DocFetcher>,
        _lost: Option<TransactionConflict>,
    ) -> MaybeBoxFuture<'static, Result<RetryAttemptValue>> {
        let (context, name, docs) = (
            self.context.clone(),
            self.collection_name.clone(),
            self.docs.clone(),
        );
        Box::pin(async move {
            let mut results = Vec::with_capacity(docs.len());
            for doc in docs {
                results.push(mutator.create_as(&context, &name, doc).await?);
            }
            Ok(boxed(results))
        })
    }
}

impl AttemptWork for UpdateWork {
    fn run(
        &self,
        mutator: Arc<dyn DocMutator>,
        fetcher: Arc<dyn query::fetcher::DocFetcher>,
        lost: Option<TransactionConflict>,
    ) -> MaybeBoxFuture<'static, Result<RetryAttemptValue>> {
        let (context, name, doc, modified) = (
            self.context.clone(),
            self.collection_name.clone(),
            self.doc.clone(),
            self.modified_fields.clone(),
        );
        let (expected, expected_fields) = (self.expected.clone(), self.expected_fields.clone());
        Box::pin(async move {
            // The caller read the document before this transaction started, so
            // check it again against this attempt's own read.
            if let Some(expected) = &expected {
                let doc_id = doc
                    .id()
                    .ok_or_else(|| QueryError::execution("update has no document ID"))?
                    .to_string();
                let ids = [doc_id];
                // Validate only what the write and the comparison depend on,
                // so a write to another field doesn't conflict with this one.
                let projected = match &expected_fields {
                    Some(fields) => {
                        let fields = fields.iter().chain(&modified).cloned().collect();
                        fetcher.get_by_ids_projected(&name, &ids, &fields).await?
                    }
                    None => None,
                };
                let current = match projected {
                    Some(current) => current,
                    None => fetcher.get_by_ids(&name, &ids).await?,
                };
                let still = current
                    .docs()
                    .first()
                    .is_some_and(|current| unchanged(expected, expected_fields.as_ref(), current));
                if !still {
                    return Err(QueryError::transaction_conflict(
                        lost.map_or(
                            storage::Error::TxnConflict,
                            storage::Error::TxnConflictDetails,
                        )
                        .to_string(),
                    ));
                }
            }
            mutator
                .update_as(&context, &name, doc, modified)
                .await
                .map(boxed)
        })
    }
}

impl AttemptWork for DeleteWork {
    fn run(
        &self,
        mutator: Arc<dyn DocMutator>,
        _fetcher: Arc<dyn query::fetcher::DocFetcher>,
        _lost: Option<TransactionConflict>,
    ) -> MaybeBoxFuture<'static, Result<RetryAttemptValue>> {
        let (context, name, doc_ids) = (
            self.context.clone(),
            self.collection_name.clone(),
            self.doc_ids.clone(),
        );
        Box::pin(async move {
            let mut results = Vec::with_capacity(doc_ids.len());
            for doc_id in doc_ids {
                results.push(mutator.delete_as(&context, &name, &doc_id).await?);
            }
            Ok(boxed(results))
        })
    }
}

impl<S> AutoCommitMutator<S>
where
    S: Store + 'static,
{
    /// Refuses user writes to chunk collections, and sends writes to collections
    /// with `@chunk` fields through a batch of one.
    pub(super) fn chunk_route(&self, collection_name: &str) -> Result<Route> {
        let Some(collection) = self
            .db
            .get_collection(collection_name)
            .map_err(|e| QueryError::execution(format!("db error: {e}")))?
        else {
            return Ok(Route::Direct);
        };
        Ok(if collection.schema().chunked_fields.is_empty() {
            Route::Direct
        } else {
            Route::Batch
        })
    }

    /// Runs `work` as one batch. Remote inputs are fetched between attempts, as on the direct path.
    pub(super) async fn in_batch<R, W>(&self, context: &RequestContext, work: W) -> Result<R>
    where
        R: Any,
        W: AttemptWork + Clone + 'static,
    {
        if self.prepared_inputs.is_some() {
            return self.batch_attempt(work).await;
        }
        let inputs = PreparedInputMemo::default();
        let mutator = Self {
            db: self.db.clone(),
            prepared_inputs: Some(inputs.clone()),
        };
        loop {
            match mutator.batch_attempt(work.clone()).await {
                Err(QueryError::NeedsInput(request)) => {
                    inputs
                        .answer_wave(&request, |wave| {
                            self.resolve_prepared_inputs_as(context, wave)
                        })
                        .await?;
                }
                result => return result,
            }
        }
    }

    /// Gives each new document its short ID up front, so every attempt uses the same IDs.
    pub(super) async fn seed_creates(
        &self,
        collection_name: &str,
        docs: &mut [Document],
    ) -> Result<()> {
        for doc in docs {
            self.seed_create_identity(collection_name, doc).await?;
        }
        Ok(())
    }

    async fn batch_attempt<R, W>(&self, work: W) -> Result<R>
    where
        R: Any,
        W: AttemptWork + 'static,
    {
        let attempt = BatchRetryAttempt {
            db: self.db.clone(),
            prepared_inputs: self.prepared_inputs.clone(),
            work,
        };
        match self
            .db
            .store()
            .transact_retry(self.db.options().retry_options(), Box::new(attempt))
            .await
        {
            Ok(value) => value
                .downcast::<R>()
                .map(|value| *value)
                .map_err(|_| QueryError::execution("invalid one-write batch result")),
            Err(error) => match error.downcast_abort::<QueryError>() {
                Ok(error) => Err(error),
                Err(error) => Err(crate::error::commit_query_error(
                    crate::error::Error::Storage(error.into_storage()),
                )),
            },
        }
    }
}
