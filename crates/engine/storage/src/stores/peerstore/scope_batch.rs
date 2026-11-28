//! Several of one peer's scope records in one transaction.
//!
//! The failure recorder takes a peer's marker registrations and
//! acknowledgements in the order they arrive and applies a run of them as one
//! transaction, so the run costs one commit, and one device sync, instead of
//! one each (#3471). Each record is the attempt its single call runs, applied
//! in order on the same transaction, so it sees what the records before it
//! wrote, as it would have seen their commits. If that transaction fails,
//! nothing of it is written and each record runs as its own single call, in
//! order, so an error costs only the record whose own call fails.

use std::ops::RangeInclusive;

use super::{Peerstore, PeerstoreRetryAttempt, RegisterScopeMarkerAttempt, ReplayMarkerWrite};
use crate::corekv::{
    Error, Result, RetryAttemptError, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue,
    RetryTxn, Store,
};

/// One record of a peer's scope, applied in the order given.
#[derive(Debug, Clone)]
pub enum ScopeRecord {
    /// Register a live head's scope marker, as
    /// [`Peerstore::observe_push_head_with_generation`].
    Observe {
        doc_id: String,
        collection_id: String,
    },
    /// Complete a scope the peer acknowledged: at `expected_generation` as
    /// [`Peerstore::complete_retry_scope`], or with none as
    /// [`Peerstore::record_acknowledged_heads`].
    Complete {
        doc_id: String,
        collection_id: String,
        is_collection: bool,
        expected_generation: Option<u64>,
        acked_heads: Vec<String>,
    },
}

/// What one [`ScopeRecord`] did, as its single call reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeOutcome {
    /// The generation the observed head's push carries, `None` without one.
    Observed(Option<u64>),
    /// Whether the completion applied.
    Completed(bool),
}

/// What [`Peerstore::record_scopes`] did with a run.
#[derive(Debug)]
pub struct ScopeRun {
    /// Each record's outcome, in the order given: what its single call
    /// returns.
    pub outcomes: Vec<Result<ScopeOutcome>>,
    /// The error preparing or committing the shared transaction, if any.
    /// The outcomes then come from individual transactions.
    pub shared_error: Option<Error>,
}

enum Step {
    NoMarker,
    Register(RegisterScopeMarkerAttempt),
    Complete(PeerstoreRetryAttempt),
}

impl Step {
    async fn apply(
        &mut self,
        txn: &mut dyn RetryTxn,
    ) -> std::result::Result<ScopeOutcome, RetryAttemptError> {
        match self {
            Self::NoMarker => Ok(ScopeOutcome::Observed(None)),
            Self::Register(attempt) => {
                let written = downcast(attempt.run(txn).await?)?;
                Ok(Self::observed(written, attempt.generation))
            }
            Self::Complete(attempt) => {
                Ok(ScopeOutcome::Completed(downcast(attempt.run(txn).await?)?))
            }
        }
    }

    fn observed(written: ReplayMarkerWrite, generation: u64) -> ScopeOutcome {
        ScopeOutcome::Observed(match written {
            ReplayMarkerWrite::Superseded => Some(generation),
            written => written.generation(),
        })
    }
}

impl RetryAttemptFn for Step {
    fn run<'attempt>(
        &'attempt mut self,
        txn: &'attempt mut dyn RetryTxn,
    ) -> RetryAttemptFuture<'attempt> {
        Box::pin(async move { Ok(Box::new(self.apply(txn).await?) as RetryAttemptValue) })
    }
}

struct ScopeObservations {
    generations: RangeInclusive<u64>,
    incarnation: Option<u64>,
    initial: Vec<u8>,
}

struct ScopeBatchAttempt {
    steps: Vec<Step>,
}

impl RetryAttemptFn for ScopeBatchAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut values = Vec::with_capacity(self.steps.len());
            for step in &mut self.steps {
                values.push(step.apply(txn).await?);
            }
            Ok(Box::new(values) as RetryAttemptValue)
        })
    }
}

fn downcast<T>(value: RetryAttemptValue) -> Result<T>
where
    T: 'static,
{
    value
        .downcast::<T>()
        .map(|value| *value)
        .map_err(|_| Error::Other("a scope record returned a value of another type".into()))
}

impl<S: Store> Peerstore<S> {
    /// Apply `records` of `peer_id`, in order, in one transaction, and return
    /// each one's outcome in the same order. Each record does what its single
    /// call does; only the commit is shared. When the shared transaction
    /// fails (a storage error, or its conflict retries run out) it writes
    /// nothing. Each prepared record then runs in its own transaction in
    /// order, retaining any incarnation and generation it already captured.
    pub async fn record_scopes(&self, peer_id: &str, records: &[ScopeRecord]) -> ScopeRun {
        let observations = match self.prepare_scope_observations(peer_id, records).await {
            Ok(observations) => observations,
            Err(error) => {
                let mut outcomes = Vec::with_capacity(records.len());
                for record in records {
                    outcomes.push(self.record_scope(peer_id, record).await);
                }
                return ScopeRun {
                    outcomes,
                    shared_error: Some(error),
                };
            }
        };
        self.record_prepared_steps(peer_id, records, observations)
            .await
    }

    async fn record_prepared_steps(
        &self,
        peer_id: &str,
        records: &[ScopeRecord],
        observations: ScopeObservations,
    ) -> ScopeRun {
        let mut generations = observations.generations.clone();
        let steps: Vec<_> = records
            .iter()
            .map(|record| self.scope_step(peer_id, record, &observations, &mut generations))
            .collect();
        if steps.iter().all(|step| matches!(step, Step::NoMarker)) {
            return ScopeRun {
                outcomes: steps
                    .iter()
                    .map(|_| Ok(ScopeOutcome::Observed(None)))
                    .collect(),
                shared_error: None,
            };
        }
        let shared = self
            .transact_conflicts::<Vec<ScopeOutcome>>(Box::new(ScopeBatchAttempt { steps }))
            .await;
        match shared {
            Ok(outcomes) => ScopeRun {
                outcomes: outcomes.into_iter().map(Ok).collect(),
                shared_error: None,
            },
            Err(error) => {
                // Reuse the original incarnation and generations after rollback.
                let mut generations = observations.generations.clone();
                let mut outcomes = Vec::with_capacity(records.len());
                for record in records {
                    let outcome =
                        match self.scope_step(peer_id, record, &observations, &mut generations) {
                            Step::NoMarker => Ok(ScopeOutcome::Observed(None)),
                            step => self.transact_conflicts(Box::new(step)).await,
                        };
                    outcomes.push(outcome);
                }
                ScopeRun {
                    outcomes,
                    shared_error: Some(error),
                }
            }
        }
    }

    /// One record as its single call, in a transaction of its own.
    async fn record_scope(&self, peer_id: &str, record: &ScopeRecord) -> Result<ScopeOutcome> {
        match record {
            ScopeRecord::Observe {
                doc_id,
                collection_id,
            } => self
                .observe_push_head_with_generation(peer_id, doc_id, collection_id)
                .await
                .map(ScopeOutcome::Observed),
            ScopeRecord::Complete {
                doc_id,
                collection_id,
                is_collection,
                expected_generation,
                acked_heads,
            } => self
                .complete_scope(
                    peer_id,
                    doc_id,
                    collection_id,
                    *is_collection,
                    *expected_generation,
                    acked_heads,
                )
                .await
                .map(ScopeOutcome::Completed),
        }
    }

    async fn prepare_scope_observations(
        &self,
        peer_id: &str,
        records: &[ScopeRecord],
    ) -> Result<ScopeObservations> {
        let observed = records
            .iter()
            .filter(|record| {
                matches!(record, ScopeRecord::Observe { doc_id, collection_id }
                    if !(doc_id.is_empty() && collection_id.is_empty()))
            })
            .count();
        let (generations, incarnation) = if observed == 0 {
            (0..=0, None)
        } else {
            let range = self
                .store
                .allocate(b"/rep/retry/marker-generation", observed as u64)
                .await?;
            // A single registration's generation is the end of a range of one.
            (
                (range.start + 1)..=range.end,
                self.get_replicator_incarnation(peer_id).await?,
            )
        };
        let initial = super::super::RetryInfo::new_initial()
            .to_bytes()
            .map_err(Error::Other)?;

        Ok(ScopeObservations {
            generations,
            incarnation,
            initial,
        })
    }

    fn scope_step(
        &self,
        peer_id: &str,
        record: &ScopeRecord,
        observations: &ScopeObservations,
        generations: &mut RangeInclusive<u64>,
    ) -> Step {
        match record {
            ScopeRecord::Observe {
                doc_id,
                collection_id,
            } => {
                let Some(expected_incarnation) = observations.incarnation else {
                    return Step::NoMarker;
                };
                if doc_id.is_empty() && collection_id.is_empty() {
                    return Step::NoMarker;
                }
                let generation = generations
                    .next()
                    .expect("one generation per scoped observation");
                Step::Register(RegisterScopeMarkerAttempt {
                    peer_id: peer_id.to_owned(),
                    expected_incarnation,
                    doc_id: doc_id.clone(),
                    collection_id: collection_id.clone(),
                    retry_info_bytes: observations.initial.clone(),
                    retry_schedule: self.retry_schedule.clone(),
                    generation,
                })
            }
            ScopeRecord::Complete {
                doc_id,
                collection_id,
                is_collection,
                expected_generation,
                acked_heads,
            } => Step::Complete(self.complete_marker_attempt(
                peer_id,
                doc_id,
                collection_id,
                *is_collection,
                *expected_generation,
                acked_heads,
            )),
        }
    }
}

#[cfg(test)]
#[path = "../peerstore_scope_batch_tests.rs"]
mod tests;
