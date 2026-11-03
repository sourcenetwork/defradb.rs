use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, FixedOffset};
use document::{Document, WritePreparation};
use kovan_map::HopscotchMap;
use kovan_queue::seg_queue::SegQueue;
use rapidhash::fast::{RandomState, RapidHashMap};

use crate::error::{QueryError, Result};

/// What requests spent waiting on external inputs, since the process started.
#[derive(Debug)]
pub struct InputWaitStats {
    /// Attempts that ended asking for inputs and ran again once they came.
    pub waves: AtomicU64,
    /// Inputs those waves fetched.
    pub inputs: AtomicU64,
    /// Attempts that ran again because an input's dependency changed under
    /// them (an embedding's source field read differently in the attempt).
    pub dependency_reruns: AtomicU64,
    /// Microseconds each wave kept its request suspended, holding no
    /// transaction.
    pub suspended_micros: inputs::Histogram,
}

static INPUT_WAITS: InputWaitStats = InputWaitStats {
    waves: AtomicU64::new(0),
    inputs: AtomicU64::new(0),
    dependency_reruns: AtomicU64::new(0),
    suspended_micros: inputs::Histogram::new(),
};

/// The input-wait metrics of this process.
pub fn input_wait_stats() -> &'static InputWaitStats {
    &INPUT_WAITS
}

/// Counts an attempt that runs again because an input's dependency changed.
pub fn record_dependency_rerun() {
    INPUT_WAITS
        .dependency_reruns
        .fetch_add(1, Ordering::Relaxed);
}

/// Resolves one wave of inputs, recording what the wait cost.
async fn timed_wave<Fut>(count: usize, wave: Fut) -> Result<Vec<Arc<[u8]>>>
where
    Fut: Future<Output = Result<Vec<Arc<[u8]>>>>,
{
    let stopwatch = inputs::Stopwatch::start();
    let answers = wave.await;
    INPUT_WAITS.waves.fetch_add(1, Ordering::Relaxed);
    INPUT_WAITS
        .inputs
        .fetch_add(count as u64, Ordering::Relaxed);
    INPUT_WAITS
        .suspended_micros
        .record(stopwatch.elapsed_micros());
    answers
}

/// A network input request, represented by its complete wire request bytes.
/// Keeping the exact bytes in the key avoids accidental reuse when any
/// dependency (including provider configuration) changes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PreparedInputRequest {
    kind: PreparedInputKind,
    request: Arc<[u8]>,
}

impl PreparedInputRequest {
    pub fn new(kind: PreparedInputKind, exact_request: impl Into<Arc<[u8]>>) -> Self {
        Self {
            kind,
            request: exact_request.into(),
        }
    }

    pub fn kind(&self) -> PreparedInputKind {
        self.kind
    }

    pub fn exact_request(&self) -> &[u8] {
        &self.request
    }
}

impl std::fmt::Debug for PreparedInputRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedInputRequest")
            .field("kind", &self.kind)
            .field("request", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreparedInputKind {
    Embedding,
    KmsKey,
    Signature,
    Encryption,
    AcpDecision,
    WriteValidation,
    NodePermission,
    ReadValidation,
    EncryptedSearch,
    /// A value an attempt drew locally (a created document's short ID) that
    /// every later attempt of the request reuses. Never asked for as a miss.
    Allocation,
}

/// Exact-request memo shared by all attempts of one logical request.
/// Values are the provider's exact response bytes; parsing happens at the
/// domain boundary that requested them.
///
/// The attempts of one request, and the tasks inside one attempt, read and
/// add answers without a lock. The first answer to a request wins and every
/// later reader gets it, so an answer drawn at random (a key, a nonce, a
/// ciphertext) is the same for every use in the request.
///
/// The memo holds only answers derived from its own request and is dropped
/// with it, so its size follows the request's and it keeps no cap of its own:
/// a cap would refuse a request that is valid only for being large. What
/// bounds a dependency miss is progress: a miss must name a request the memo
/// does not hold yet ([`answer_miss`](Self::answer_miss)).
///
/// A pass that meets several misses goes on past them and records each
/// ([`want`](Self::want)), so the wave that answers the miss ending the
/// attempt answers them all at once ([`answer_wave`](Self::answer_wave)):
/// the attempts of a request make one round trip per wave of inputs, not one
/// per input.
#[derive(Clone)]
pub struct PreparedInputMemo {
    answers: Arc<HopscotchMap<PreparedInputRequest, Arc<[u8]>, RandomState>>,
    wanted: Arc<SegQueue<PreparedInputRequest>>,
}

impl Default for PreparedInputMemo {
    fn default() -> Self {
        Self {
            answers: Arc::new(HopscotchMap::with_hasher(RandomState::default())),
            wanted: Arc::new(SegQueue::new()),
        }
    }
}

impl std::fmt::Debug for PreparedInputMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedInputMemo")
            .field("entries", &self.len())
            .field("request_bytes", &"[redacted]")
            .field("response_bytes", &"[redacted]")
            .finish()
    }
}

impl PreparedInputMemo {
    pub fn get(&self, request: &PreparedInputRequest) -> Option<Arc<[u8]>> {
        self.answers.get(request)
    }

    /// The answer to `request`, or, when the memo holds none, a note that the
    /// request is wanted: the pass goes on and the next wave answers it.
    pub fn get_or_want(&self, request: &PreparedInputRequest) -> Option<Arc<[u8]>> {
        let answer = self.get(request);
        if answer.is_none() {
            self.want(request.clone());
        }
        answer
    }

    /// Records a request a pass found unanswered and went on past.
    pub fn want(&self, request: PreparedInputRequest) {
        self.wanted.push(request);
    }

    /// Memoize `response` unless the request is already answered, and return
    /// the answer every use of the request must share: the first one.
    pub fn insert(
        &self,
        request: PreparedInputRequest,
        response: impl Into<Arc<[u8]>>,
    ) -> Arc<[u8]> {
        self.answers.get_or_insert(request, response.into())
    }

    /// Answer the dependency miss that ended an attempt, so the next attempt
    /// finds the input local. `resolve` fetches it outside any transaction.
    /// A miss for a request the memo already answers means the attempt reads
    /// some other memo, and running it again would only miss again: that is
    /// an error, not another round.
    pub async fn answer_miss<F, Fut>(
        &self,
        request: &PreparedInputRequest,
        resolve: F,
    ) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<[u8]>>>,
    {
        if self.answers.contains_key(request) {
            return Err(QueryError::execution(format!(
                "external input {:?} was requested again after it was provided",
                request.kind()
            )));
        }
        let response = timed_wave(1, async { resolve().await.map(|answer| vec![answer]) })
            .await?
            .pop()
            .ok_or_else(|| QueryError::execution("external input went unanswered"))?;
        self.insert(request.clone(), response);
        Ok(())
    }

    /// Answers the miss that ended an attempt together with every request
    /// the attempt went on past, as one wave: `resolve` gets them all, each
    /// once, and returns their answers in order, fetched outside any
    /// transaction. As with [`answer_miss`](Self::answer_miss), a miss for a
    /// request already answered is an error.
    pub async fn answer_wave<F, Fut>(
        &self,
        request: &PreparedInputRequest,
        resolve: F,
    ) -> Result<()>
    where
        F: FnOnce(Vec<PreparedInputRequest>) -> Fut,
        Fut: Future<Output = Result<Vec<Arc<[u8]>>>>,
    {
        if self.answers.contains_key(request) {
            return Err(QueryError::execution(format!(
                "external input {:?} was requested again after it was provided",
                request.kind()
            )));
        }
        let mut wave = vec![request.clone()];
        let mut seen = rapidhash::fast::RapidHashSet::default();
        seen.insert(request.clone());
        while let Some(wanted) = self.wanted.pop() {
            if !self.answers.contains_key(&wanted) && seen.insert(wanted.clone()) {
                wave.push(wanted);
            }
        }
        let answers = timed_wave(wave.len(), resolve(wave.clone())).await?;
        if answers.len() != wave.len() {
            return Err(QueryError::execution(format!(
                "asked for {} external inputs, got {} answers",
                wave.len(),
                answers.len()
            )));
        }
        for (request, answer) in wave.into_iter().zip(answers) {
            self.insert(request, answer);
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.answers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Immutable write inputs shared by attempts of one mutation request.
#[derive(Debug, Clone)]
pub struct PreparedMutations {
    pub request_time: DateTime<FixedOffset>,
    pub mutations: RapidHashMap<String, PreparedMutation>,
    pub inputs: PreparedInputMemo,
    /// The selected implicit batch controller owns commit-conflict retries.
    pub engine_managed_retry: bool,
    /// The request this capsule was prepared from, parsed once: each
    /// execution of the request uses a copy instead of parsing it again.
    pub parsed: Option<Arc<crate::query_parse::ParsedOperation>>,
}

impl Default for PreparedMutations {
    fn default() -> Self {
        Self {
            request_time: chrono::Utc::now().fixed_offset(),
            mutations: RapidHashMap::default(),
            inputs: PreparedInputMemo::default(),
            engine_managed_retry: false,
            parsed: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PreparedMutation {
    pub creates: Vec<Document>,
    /// Existing targets observed and matched during write preparation. This
    /// is separate from `updates`: some targets need no key payload, but a
    /// retry must still reject targets that appeared after preparation.
    pub update_targets: rapidhash::RapidHashSet<String>,
    pub updates: RapidHashMap<String, Arc<WritePreparation>>,
    pub embeddings: RapidHashMap<String, Vec<PreparedEmbedding>>,
    pub doc_ids: Option<Vec<String>>,
}

impl PreparedMutation {
    /// Rebind what preparation recorded for documents this request created to
    /// the IDs their creates returned. `created` maps the ID preparation saw a
    /// created document under to its actual ID.
    ///
    /// A document the request created is a known target of its later
    /// mutations, whether preparation found it under its projected ID or the
    /// mutation names it by the ID its create returned, which preparation
    /// could not see. Inputs nobody prepared for it arrive as input misses
    /// when the mutation runs.
    pub(crate) fn bind_created(&mut self, created: &RapidHashMap<String, String>) {
        for (projected, actual) in created {
            self.update_targets.remove(projected);
            self.update_targets.insert(actual.clone());
            if let Some(write) = self.updates.remove(projected) {
                self.updates.insert(actual.clone(), write);
            }
            if let Some(embeddings) = self.embeddings.remove(projected) {
                self.embeddings.insert(actual.clone(), embeddings);
            }
        }
    }
}

/// An embedding computed during request preparation together with the exact
/// source values it consumed. Attempts reuse the vector only while these
/// dependencies still match their current document.
#[derive(Debug, Clone)]
pub struct PreparedEmbedding {
    pub field: String,
    pub value: document::NormalValue,
    pub dependencies: RapidHashMap<String, document::NormalValue>,
    pub request: PreparedInputRequest,
}

pub(crate) fn prepared_doc_id(write: &WritePreparation) -> document::DocID {
    document::DocID::new_v0_from_seed(&format!(
        "key-preparation/{}/{}",
        write.collection_short_id, write.doc_short_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_input_memo_reuses_only_identical_wire_requests() {
        let memo = PreparedInputMemo::default();
        let mut provider_calls = 0;
        let resolve =
            |memo: &PreparedInputMemo, request: PreparedInputRequest, calls: &mut usize| {
                if let Some(response) = memo.get(&request) {
                    return response;
                }
                *calls += 1;
                memo.insert(request, b"provider response".as_slice())
            };

        let first = PreparedInputRequest::new(
            PreparedInputKind::Embedding,
            br#"{"model":"m1","input":"source A"}"#.as_slice(),
        );
        let first_response = resolve(&memo, first.clone(), &mut provider_calls);
        let retry_response = resolve(&memo, first, &mut provider_calls);
        assert_eq!(&*first_response, &*retry_response);
        assert_eq!(provider_calls, 1);

        // A changed source field or model changes the exact wire bytes and
        // therefore cannot reuse an embedding from the previous dependency set.
        let changed = PreparedInputRequest::new(
            PreparedInputKind::Embedding,
            br#"{"model":"m1","input":"source B"}"#.as_slice(),
        );
        resolve(&memo, changed, &mut provider_calls);
        let changed_model = PreparedInputRequest::new(
            PreparedInputKind::Embedding,
            br#"{"model":"m2","input":"source B"}"#.as_slice(),
        );
        resolve(&memo, changed_model, &mut provider_calls);
        assert_eq!(provider_calls, 3);
        assert_eq!(memo.len(), 3);
    }

    #[test]
    fn prepared_input_request_debug_redacts_wire_bytes() {
        let request = PreparedInputRequest::new(
            PreparedInputKind::KmsKey,
            b"secret-dependency-and-provider-context".as_slice(),
        );
        let debug = format!("{request:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("secret-dependency"));
    }

    /// A request that derives thousands of inputs and tens of megabytes of
    /// answers keeps every one: the memo follows its request's size.
    #[test]
    fn a_large_request_keeps_every_input() {
        let memo = PreparedInputMemo::default();
        let answer = vec![7_u8; 4096];
        for index in 0_u32..8192 {
            let request = PreparedInputRequest::new(
                PreparedInputKind::Embedding,
                index.to_be_bytes().to_vec(),
            );
            memo.insert(request, answer.as_slice());
        }
        assert_eq!(memo.len(), 8192);
        let last = PreparedInputRequest::new(
            PreparedInputKind::Embedding,
            8191_u32.to_be_bytes().to_vec(),
        );
        assert_eq!(memo.get(&last).as_deref(), Some(answer.as_slice()));
    }

    #[test]
    fn the_first_answer_to_a_request_is_the_one_every_use_shares() {
        let memo = PreparedInputMemo::default();
        let request = PreparedInputRequest::new(PreparedInputKind::Encryption, b"nonce".as_slice());
        assert_eq!(
            &*memo.insert(request.clone(), b"first".as_slice()),
            b"first"
        );
        assert_eq!(
            &*memo.insert(request.clone(), b"second".as_slice()),
            b"first"
        );
        assert_eq!(memo.get(&request).as_deref(), Some(b"first".as_slice()));
    }

    #[test]
    fn a_created_document_is_a_known_target_under_its_actual_id() {
        let mut mutation = PreparedMutation::default();
        let write = Arc::new(WritePreparation::new(1, 7));
        mutation.update_targets.insert("projected-a".to_string());
        mutation
            .updates
            .insert("projected-a".to_string(), Arc::clone(&write));
        mutation.update_targets.insert("existing".to_string());

        let mut created = RapidHashMap::default();
        created.insert("projected-a".to_string(), "actual-a".to_string());
        // Preparation never saw this one: a later mutation named it by the ID
        // its create returned.
        created.insert("projected-b".to_string(), "actual-b".to_string());
        mutation.bind_created(&created);

        assert!(mutation.update_targets.contains("actual-a"));
        assert!(mutation.update_targets.contains("actual-b"));
        assert!(mutation.update_targets.contains("existing"));
        assert!(!mutation.update_targets.contains("projected-a"));
        assert!(!mutation.update_targets.contains("projected-b"));
        assert!(mutation
            .updates
            .get("actual-a")
            .is_some_and(|bound| Arc::ptr_eq(bound, &write)));
        assert!(!mutation.updates.contains_key("projected-a"));
        assert!(!mutation.updates.contains_key("actual-b"));
    }

    #[test]
    fn a_miss_is_answered_once_and_asking_again_is_an_error() {
        workers::block_on(async {
            let memo = PreparedInputMemo::default();
            let request =
                PreparedInputRequest::new(PreparedInputKind::Signature, b"block".as_slice());
            let calls = std::sync::atomic::AtomicUsize::new(0);
            let resolve = || async {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(Arc::<[u8]>::from(b"signature".as_slice()))
            };

            memo.answer_miss(&request, resolve).await.unwrap();
            assert_eq!(memo.get(&request).as_deref(), Some(b"signature".as_slice()));

            let error = memo.answer_miss(&request, resolve).await.unwrap_err();
            assert!(error.to_string().contains("requested again"), "{error}");
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        })
        .unwrap()
    }

    #[test]
    fn a_wave_answers_the_miss_and_every_request_the_pass_went_past() {
        workers::block_on(async {
            let memo = PreparedInputMemo::default();
            let ended =
                PreparedInputRequest::new(PreparedInputKind::Signature, b"composite".as_slice());
            let passed =
                PreparedInputRequest::new(PreparedInputKind::Signature, b"field".as_slice());
            assert!(memo.get_or_want(&passed).is_none());
            assert!(
                memo.get_or_want(&passed).is_none(),
                "asked twice, wanted once"
            );
            let waves = std::sync::atomic::AtomicUsize::new(0);
            memo.answer_wave(&ended, |wave| {
                waves.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    assert_eq!(wave.len(), 2);
                    Ok(wave
                        .iter()
                        .map(|request| Arc::<[u8]>::from(request.exact_request()))
                        .collect())
                }
            })
            .await
            .unwrap();
            assert_eq!(waves.load(std::sync::atomic::Ordering::SeqCst), 1);
            assert_eq!(memo.get(&ended).as_deref(), Some(b"composite".as_slice()));
            assert_eq!(memo.get(&passed).as_deref(), Some(b"field".as_slice()));

            let error = memo
                .answer_wave(&ended, |wave| async move {
                    Ok(wave
                        .iter()
                        .map(|_| Arc::<[u8]>::from(b"".as_slice()))
                        .collect())
                })
                .await
                .unwrap_err();
            assert!(error.to_string().contains("requested again"), "{error}");
        })
        .unwrap()
    }

    #[test]
    fn a_wave_records_its_inputs_and_how_long_it_kept_the_request_suspended() {
        workers::block_on(async {
            let stats = input_wait_stats();
            let (waves, inputs, suspended) = (
                stats.waves.load(Ordering::Relaxed),
                stats.inputs.load(Ordering::Relaxed),
                stats.suspended_micros.count(),
            );
            let memo = PreparedInputMemo::default();
            let ended = PreparedInputRequest::new(PreparedInputKind::Embedding, b"one".as_slice());
            memo.want(PreparedInputRequest::new(
                PreparedInputKind::Embedding,
                b"two".as_slice(),
            ));
            memo.answer_wave(&ended, |wave| async move {
                Ok(wave
                    .iter()
                    .map(|request| Arc::<[u8]>::from(request.exact_request()))
                    .collect())
            })
            .await
            .unwrap();
            // Other tests of this binary record too: the counters only grow.
            assert!(stats.waves.load(Ordering::Relaxed) > waves);
            assert!(stats.inputs.load(Ordering::Relaxed) >= inputs + 2);
            assert!(stats.suspended_micros.count() > suspended);
        })
        .unwrap()
    }

    #[test]
    fn a_dependency_rerun_is_counted() {
        let before = input_wait_stats().dependency_reruns.load(Ordering::Relaxed);
        record_dependency_rerun();
        assert!(input_wait_stats().dependency_reruns.load(Ordering::Relaxed) > before);
    }
}
