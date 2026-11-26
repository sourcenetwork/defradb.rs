//! Several of one peer's scope records in one transaction.
//!
//! The failure recorder takes a peer's marker registrations and
//! acknowledgements in the order they arrive and applies a run of them as one
//! transaction, so the run costs one commit, and one device sync, instead of
//! one each (#3471). Each record is the attempt its single call runs, applied
//! in order on the same transaction, so it sees what the records before it
//! wrote, as it would have seen their commits.

use super::{
    decode_retry_marker_generation, Peerstore, PeerstoreRetryAttempt, RegisterScopeMarkerAttempt,
    ReplayMarkerWrite,
};
use crate::corekv::{
    Error, Key, Reader, Result, RetryAttemptFn, RetryAttemptFuture, RetryAttemptValue, RetryTxn,
    Store,
};
use crate::keys::peerstore::{ReplicatorIncarnationKey, ReplicatorKey};

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

/// Where a record's outcome comes from: decided before the transaction, or
/// the value of its step.
enum Slot {
    Decided(ScopeOutcome),
    Observe { step: usize, generation: u64 },
    Complete { step: usize },
}

enum Step {
    Register(RegisterScopeMarkerAttempt),
    Complete(PeerstoreRetryAttempt),
}

/// What a step returned.
enum StepValue {
    Register(ReplayMarkerWrite),
    Complete(bool),
}

struct ScopeBatchAttempt {
    steps: Vec<Step>,
}

impl RetryAttemptFn for ScopeBatchAttempt {
    fn run<'a>(&'a mut self, txn: &'a mut dyn RetryTxn) -> RetryAttemptFuture<'a> {
        Box::pin(async move {
            let mut values = Vec::with_capacity(self.steps.len());
            for step in &mut self.steps {
                let value = match step {
                    Step::Register(attempt) => {
                        StepValue::Register(downcast(attempt.run(&mut *txn).await?)?)
                    }
                    Step::Complete(attempt) => {
                        StepValue::Complete(downcast(attempt.run(&mut *txn).await?)?)
                    }
                };
                values.push(value);
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
    /// call does; only the commit is shared.
    pub async fn record_scopes(
        &self,
        peer_id: &str,
        records: &[ScopeRecord],
    ) -> Result<Vec<ScopeOutcome>> {
        let observed = records
            .iter()
            .filter(|record| {
                matches!(record, ScopeRecord::Observe { doc_id, collection_id }
                    if !(doc_id.is_empty() && collection_id.is_empty()))
            })
            .count();
        let mut generations = if observed == 0 {
            0..0
        } else {
            let range = self
                .store
                .allocate(b"/rep/retry/marker-generation", observed as u64)
                .await?;
            // A single registration's generation is the end of a range of
            // one, so a run takes the values just above each start.
            (range.start + 1)..(range.end + 1)
        };
        let incarnation = if observed == 0 {
            None
        } else {
            self.peer_incarnation(peer_id).await?
        };
        let initial = super::super::RetryInfo::new_initial()
            .to_bytes()
            .map_err(Error::Other)?;

        let mut slots = Vec::with_capacity(records.len());
        let mut steps = Vec::with_capacity(records.len());
        for record in records {
            match record {
                ScopeRecord::Observe {
                    doc_id,
                    collection_id,
                } => {
                    if doc_id.is_empty() && collection_id.is_empty() {
                        slots.push(Slot::Decided(ScopeOutcome::Observed(None)));
                        continue;
                    }
                    let generation = generations
                        .next()
                        .ok_or_else(|| Error::Other("too few marker generations".into()))?;
                    let Some(expected_incarnation) = incarnation else {
                        slots.push(Slot::Decided(ScopeOutcome::Observed(None)));
                        continue;
                    };
                    slots.push(Slot::Observe {
                        step: steps.len(),
                        generation,
                    });
                    steps.push(Step::Register(RegisterScopeMarkerAttempt {
                        peer_id: peer_id.to_owned(),
                        expected_incarnation,
                        doc_id: doc_id.clone(),
                        collection_id: collection_id.clone(),
                        retry_info_bytes: initial.clone(),
                        retry_schedule: self.retry_schedule.clone(),
                        generation,
                    }));
                }
                ScopeRecord::Complete {
                    doc_id,
                    collection_id,
                    is_collection,
                    expected_generation,
                    acked_heads,
                } => {
                    slots.push(Slot::Complete { step: steps.len() });
                    steps.push(Step::Complete(self.complete_marker_attempt(
                        peer_id,
                        doc_id,
                        collection_id,
                        *is_collection,
                        *expected_generation,
                        acked_heads,
                    )));
                }
            }
        }

        let values: Vec<StepValue> = if steps.is_empty() {
            Vec::new()
        } else {
            self.transact_conflicts(Box::new(ScopeBatchAttempt { steps }))
                .await?
        };
        slots
            .into_iter()
            .map(|slot| match slot {
                Slot::Decided(outcome) => Ok(outcome),
                // A newer marker already owes this head, as
                // register_scope_marker_at reports it.
                Slot::Observe { step, generation } => match values.get(step) {
                    Some(StepValue::Register(ReplayMarkerWrite::Superseded)) => {
                        Ok(ScopeOutcome::Observed(Some(generation)))
                    }
                    Some(StepValue::Register(written)) => {
                        Ok(ScopeOutcome::Observed(written.generation()))
                    }
                    _ => Err(Error::Other("a scope record has no value".into())),
                },
                Slot::Complete { step } => match values.get(step) {
                    Some(StepValue::Complete(completed)) => Ok(ScopeOutcome::Completed(*completed)),
                    _ => Err(Error::Other("a scope record has no value".into())),
                },
            })
            .collect()
    }

    /// The peer's incarnation if it is configured, read at one snapshot as
    /// [`Self::register_scope_marker_at`] reads it.
    async fn peer_incarnation(&self, peer_id: &str) -> Result<Option<u64>> {
        let txn = self.store.new_txn(true).await?;
        if txn
            .get(&ReplicatorKey::new(peer_id).bytes())
            .await?
            .is_none()
        {
            return Ok(None);
        }
        match txn
            .get(&ReplicatorIncarnationKey::new(peer_id).bytes())
            .await?
        {
            Some(bytes) => decode_retry_marker_generation(&bytes)
                .map(Some)
                .ok_or_else(|| Error::Other("invalid peer incarnation".into())),
            None => Ok(Some(0)),
        }
    }
}
