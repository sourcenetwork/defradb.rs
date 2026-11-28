use std::sync::Arc;

use query::error::{QueryError, Result};
use query::prepared::{PreparedInputMemo, PreparedInputRequest};
use storage::corekv::Store;

use crate::database::DB;
use crate::remote_inputs::{Embedding, EMBEDDING_BATCH_SIZE, EMBEDDING_FLIGHTS};
use crate::search::embedding::EmbeddingInput;

/// Give each bounded batch its own wait budget. Drain the batches already
/// started together before returning an error, so their answers reach the memo.
pub(super) async fn resolve_embeddings<S>(
    db: &DB<S>,
    memo: Option<&PreparedInputMemo>,
    requests: &[&PreparedInputRequest],
) -> Result<Vec<Arc<[u8]>>>
where
    S: Store + 'static,
{
    let mut answers = Vec::with_capacity(requests.len());
    for window in requests.chunks(EMBEDDING_BATCH_SIZE * EMBEDDING_FLIGHTS) {
        let batches = futures::future::join_all(
            window
                .chunks(EMBEDDING_BATCH_SIZE)
                .map(|batch| resolve_embedding_batch(db, memo, batch)),
        )
        .await;
        for batch in batches {
            answers.extend(batch?);
        }
    }
    Ok(answers)
}

pub(super) async fn resolve_embedding_batch<S>(
    db: &DB<S>,
    memo: Option<&PreparedInputMemo>,
    requests: &[&PreparedInputRequest],
) -> Result<Vec<Arc<[u8]>>>
where
    S: Store + 'static,
{
    let mut answers = Vec::with_capacity(requests.len());
    let mut pending = Vec::new();
    let mut keys = Vec::new();
    for (index, &request) in requests.iter().enumerate() {
        let answer = memo.and_then(|memo| memo.get(request));
        if answer.is_none() {
            let input = EmbeddingInput::decode(request.exact_request())
                .map_err(|error| QueryError::execution(format!("embedding error: {error}")))?;
            keys.push(Embedding::new(input));
            pending.push((index, request));
        }
        answers.push(answer);
    }
    let vectors = if keys.is_empty() {
        Vec::new()
    } else {
        db.remote_inputs().embeddings().get_many(keys).await
    };
    let mut first_error = None;
    for ((index, request), vector) in pending.into_iter().zip(vectors) {
        let answer = vector
            .map_err(|error| QueryError::execution(format!("embedding error: {error}")))
            .and_then(|vector| {
                serde_json::to_vec(&*vector)
                    .map(Arc::<[u8]>::from)
                    .map_err(|error| QueryError::execution(error.to_string()))
            });
        match answer {
            Ok(answer) => {
                answers[index] = Some(match memo {
                    Some(memo) => memo.insert(request.clone(), answer),
                    None => answer,
                });
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => answers
            .into_iter()
            .map(|answer| {
                answer.ok_or_else(|| QueryError::execution("embedding service returned nothing"))
            })
            .collect(),
    }
}
