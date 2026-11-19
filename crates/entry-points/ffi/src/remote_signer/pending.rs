//! Signatures a host is making for its asynchronous callback, each waiting
//! for its one result.
//!
//! Every request is a token: the key of an entry in one process-wide
//! lock-free map, naming the wave the request belongs to and its place
//! there. [`defra_remote_sign_complete`] removes the entry, so a result lands
//! once; a second report of the token, or one that arrives after its wave
//! stopped waiting, finds no entry and is refused. A wave counts its results
//! down on a `Latch` and suspends on it, holding no thread.

use std::ffi::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use defra_core::signing::SignFailure;
use rapidhash::fast::RandomState;
use regolith::sync::{HopscotchMap, Latch, Lazy, OnceCell};

use super::callback::{failed, signature};

type Signed = Result<Vec<u8>, SignFailure>;

/// Requests waiting for their result, by token.
static PENDING: Lazy<HopscotchMap<usize, Request, RandomState>> =
    Lazy::new(|| HopscotchMap::with_hasher(RandomState::default()));

/// The next token. 0 is never one.
static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);

/// Where one request's result goes.
#[derive(Clone)]
struct Request {
    results: Arc<Results>,
    index: usize,
}

/// One result per request of a wave, and how many are still out.
struct Results {
    slots: Box<[OnceCell<Signed>]>,
    outstanding: Latch,
}

impl Results {
    fn settle(&self, index: usize, result: Signed) {
        if self.slots[index].set(result).is_ok() {
            self.outstanding.count_down();
        }
    }
}

/// The requests of one wave. Dropping it withdraws those still out, so a
/// wave that stopped waiting leaves nothing behind.
pub(super) struct Wave {
    results: Arc<Results>,
    tokens: Vec<usize>,
}

impl Wave {
    /// A wave of `len` requests, none opened yet.
    pub(super) fn new(len: usize) -> Self {
        Self {
            results: Arc::new(Results {
                slots: (0..len).map(|_| OnceCell::new()).collect(),
                outstanding: Latch::new(len),
            }),
            tokens: Vec::with_capacity(len),
        }
    }

    /// Opens request `index`, whose result is reported under the returned
    /// token.
    pub(super) fn open(&mut self, index: usize) -> usize {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        PENDING.insert(
            token,
            Request {
                results: Arc::clone(&self.results),
                index,
            },
        );
        self.tokens.push(token);
        token
    }

    /// Fails the request `token` with `failure`, unless its result is in.
    pub(super) fn fail(&self, token: usize, failure: SignFailure) {
        if let Some(request) = PENDING.remove(&token) {
            request.results.settle(request.index, Err(failure));
        }
    }

    /// Every result, in request order, once the last one is in.
    pub(super) async fn results(self) -> Vec<Signed> {
        self.results.outstanding.wait().await;
        self.results
            .slots
            .iter()
            .map(|slot| {
                slot.get().cloned().unwrap_or_else(|| {
                    Err(SignFailure::refused("the signature was never reported"))
                })
            })
            .collect()
    }
}

impl Drop for Wave {
    fn drop(&mut self) {
        for token in &self.tokens {
            PENDING.remove(token);
        }
    }
}

/// Report the result of a signature that a `DefraRemoteSignAsyncCallback`
/// started.
///
/// Call it exactly once per request, from any thread, at any time after the
/// callback was called (also from inside it).
///
/// - `completion` is the value the callback received.
/// - `status` is `0` when the payload was signed, `DEFRA_SIGN_RETRYABLE`
///   when the request did not reach the signer or its answer did not come
///   back (DefraDB may ask again), and any other value when the signer
///   refused.
/// - `signature_ptr` / `signature_len` are the signature when `status` is
///   `0`; the bytes are copied before this returns.
///
/// Returns `0` when the result was delivered, and `1` when no request waits
/// under `completion`: it was reported already, or the write stopped waiting
/// for it (its deadline passed).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_remote_sign_complete(
    completion: usize,
    status: c_int,
    signature_ptr: *const u8,
    signature_len: usize,
) -> c_int {
    let delivered = catch_unwind(AssertUnwindSafe(|| {
        let Some(request) = PENDING.remove(&completion) else {
            return false;
        };
        let result = if status != 0 {
            Err(failed(status, true))
        } else if signature_ptr.is_null() {
            signature(0, Vec::new)
        } else {
            signature(signature_len, || {
                // SAFETY: the host passes `signature_len` readable bytes at
                // `signature_ptr`, which is non-null; the length is bounded
                // by the check before this read.
                unsafe { std::slice::from_raw_parts(signature_ptr, signature_len) }.to_vec()
            })
        };
        request.results.settle(request.index, result);
        true
    }));
    match delivered {
        Ok(true) => 0,
        Ok(false) | Err(_) => 1,
    }
}
