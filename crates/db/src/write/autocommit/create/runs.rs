//! A batch's document blocks computed in parallel: the documents are split
//! into runs spread over a pool's workers and the calling task, which
//! computes its share while the workers compute theirs, one run per poll.
//! Each run is moved to where it is computed and comes back with its blocks,
//! so no document is copied, each worker is handed its runs with one wake,
//! and the results keep the documents' order.

use std::sync::Arc;

use defra_core::RequestContext;
use document::Document;
use query::prepared::PreparedInputMemo;
use schema::CollectionVersion;

use crate::block::builder::{
    compute_document_blocks_with_inputs, BuildError, ComputedBlocks, DocStorageIdentity,
};

/// Documents per run at most: a few milliseconds of compute, so a run never
/// holds a worker near its stuck limit.
const RUN: usize = 32;

/// Documents every lane must be given before any run leaves this task: a run
/// handed to a worker costs a wake and a hand-off each way. Measured on
/// 16-field documents, 8 lanes, mimalloc, against computing every document
/// here: from 4 documents a lane the fan-out was faster in every round at 32,
/// 128 and 256 documents and in two of three at 64; below it, not reliably
/// (2 slower, 4 to 16 mixed). A batch under `MIN_RUN` a lane is computed here.
const MIN_RUN: usize = 4;

/// A document and its blocks, or why they could not be built.
pub(super) type Computed = (Document, Result<ComputedBlocks, BuildError>);

/// Every document's blocks, in the documents' order: the runs of every lane
/// but the first on `pool`'s workers, the first lane's here; all of them here
/// without a pool.
pub(super) async fn compute_blocks(
    pool: Option<&workers::Pool>,
    docs: Vec<(Document, DocStorageIdentity)>,
    schema: &Arc<CollectionVersion>,
    context: &RequestContext,
    memo: Option<&PreparedInputMemo>,
) -> query::error::Result<Vec<Computed>> {
    let count = docs.len();
    let lanes = pool.map_or(0, workers::Pool::workers) + 1;
    let lanes = if count < lanes * MIN_RUN { 1 } else { lanes };
    let runs = count.div_ceil(RUN).div_ceil(lanes) * lanes;
    let per_run = count.div_ceil(runs.max(1)).max(1);
    let mut docs = docs.into_iter();
    let mut here = Vec::new();
    let mut there = Vec::new();
    let mut order = Vec::with_capacity(runs);
    for index in 0..runs {
        let run: Vec<_> = docs.by_ref().take(per_run).collect();
        if run.is_empty() {
            break;
        }
        let lane = index % lanes;
        order.push(lane == 0);
        if lane == 0 {
            here.push(run);
            continue;
        }
        let (schema, context, memo) = (Arc::clone(schema), context.clone(), memo.cloned());
        let job = move || async move { compute_run(run, &schema, &context, memo.as_ref()) };
        there.push((workers::Target::Worker(lane - 1), job));
    }
    let pending = match pool {
        Some(pool) if !there.is_empty() => pool.submit_batch(there).await,
        _ => Vec::new(),
    };
    let mut here = here.into_iter();
    let mut pending = pending.into_iter();
    let failed = |error: workers::Error| {
        query::error::QueryError::execution(format!("block computation task failed: {error}"))
    };
    let mut computed = Vec::with_capacity(count);
    let last = order.len().saturating_sub(1);
    for (index, is_here) in order.into_iter().enumerate() {
        let run = if is_here {
            let run = here
                .next()
                .map(|run| compute_run(run, schema, context, memo));
            if index < last {
                // One run per poll: the other tasks of this core run between.
                workers::yield_now().await;
            }
            run
        } else {
            match pending.next() {
                Some(completion) => Some(completion.map_err(failed)?.await.map_err(failed)?),
                None => None,
            }
        };
        computed.extend(run.into_iter().flatten());
    }
    if computed.len() != count {
        return Err(query::error::QueryError::execution(format!(
            "block computation returned {} of {count} documents",
            computed.len()
        )));
    }
    Ok(computed)
}

fn compute_run(
    run: Vec<(Document, DocStorageIdentity)>,
    schema: &CollectionVersion,
    context: &RequestContext,
    memo: Option<&PreparedInputMemo>,
) -> Vec<Computed> {
    run.into_iter()
        .map(|(doc, identity)| {
            let blocks = compute_document_blocks_with_inputs(
                &doc,
                schema,
                identity,
                context.encryption(),
                context.signing(),
                memo,
            );
            (doc, blocks)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use document::NormalValue;

    use super::*;

    fn documents(count: usize) -> Vec<(Document, DocStorageIdentity)> {
        (0..count)
            .map(|at| {
                let mut doc = Document::new(crate::common::schema::open_collection(&["name"]));
                doc.set("name", NormalValue::String(format!("user-{at}")))
                    .unwrap();
                (doc, DocStorageIdentity::new(1, at as u64 + 1))
            })
            .collect()
    }

    fn tasks(pool: &workers::Pool) -> Vec<u64> {
        pool.stats()
            .workers
            .iter()
            .map(|worker| worker.tasks)
            .collect()
    }

    /// The blocks `compute_blocks` returned for `count` documents, checked
    /// against each document computed alone, and the tasks each worker of
    /// `pool` ran for them.
    async fn computed_with(pool: &workers::Pool, count: usize) -> Vec<u64> {
        let context = RequestContext::ambient();
        let before = tasks(pool);
        let schema = Arc::new(CollectionVersion::new("Users", "schema", "users", vec![]));
        let computed = compute_blocks(Some(pool), documents(count), &schema, &context, None)
            .await
            .unwrap();
        let ran = tasks(pool)
            .iter()
            .zip(&before)
            .map(|(after, before)| after - before)
            .collect();
        assert_eq!(computed.len(), count);
        for ((doc, blocks), (expected_doc, identity)) in computed.iter().zip(documents(count)) {
            let expected = compute_document_blocks_with_inputs(
                &expected_doc,
                &schema,
                identity,
                None,
                None,
                None,
            )
            .unwrap();
            assert_eq!(doc, &expected_doc);
            let blocks = blocks.as_ref().unwrap();
            assert_eq!(blocks.block_result.cid, expected.block_result.cid);
            assert_eq!(blocks.blockstore_entries, expected.blockstore_entries);
        }
        ran
    }

    /// Four workers and this task: five lanes.
    const LANES: usize = 5;

    #[test]
    fn a_batch_short_of_a_full_run_for_every_lane_is_computed_here() {
        workers::block_on(async {
            let pool = workers::Pool::start(workers::Config::unplaced(LANES - 1)).unwrap();
            let ran = computed_with(&pool, LANES * MIN_RUN - 1).await;
            assert!(ran.iter().all(|tasks| *tasks == 0), "{ran:?}");
            pool.shutdown_blocking().unwrap();
        })
        .unwrap()
    }

    #[test]
    fn a_batch_with_a_full_run_for_every_lane_reaches_every_worker() {
        workers::block_on(async {
            let pool = workers::Pool::start(workers::Config::unplaced(LANES - 1)).unwrap();
            let ran = computed_with(&pool, LANES * MIN_RUN).await;
            assert!(ran.iter().all(|tasks| *tasks > 0), "{ran:?}");
            pool.shutdown_blocking().unwrap();
        })
        .unwrap()
    }
}
