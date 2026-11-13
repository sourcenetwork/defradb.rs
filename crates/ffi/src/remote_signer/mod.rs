//! Signing keys the host keeps: the C callbacks a host registers to sign with
//! a key DefraDB never sees (a Secure Enclave key, a remote identity
//! provider), and the [`RemoteSigner`] each becomes.
//!
//! A host signs in one of two forms (D37):
//! - synchronously ([`DefraRemoteSignCallback`]): the callback signs before
//!   it returns, blocking its thread, so a wave of signatures runs it on the
//!   network runtime's blocking threads, apart from the task that awaits the
//!   wave;
//! - asynchronously ([`DefraRemoteSignAsyncCallback`]): the callback starts a
//!   signature and returns at once, and the host reports the result later,
//!   from any thread, through [`defra_remote_sign_complete`]. A wave starts
//!   every signature at once and completes when the last result arrives,
//!   holding no thread meanwhile, so it runs with a single worker.
//!
//! What must never happen: a result delivered twice, or to another request;
//! a request left behind once its wave stopped waiting; a synchronous
//! callback run on the task that awaits a wave.

mod callback;
mod pending;
mod register;
#[cfg(test)]
mod registration_tests;
#[cfg(test)]
mod tests;

use std::ffi::CString;
use std::future::Future;
use std::sync::Arc;

use defra_core::signing::{HostContext, RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;

pub use callback::{DefraRemoteSignAsyncCallback, DefraRemoteSignCallback, DEFRA_SIGN_RETRYABLE};
pub use pending::defra_remote_sign_complete;
pub use register::{
    defra_identity_new_remote, defra_identity_new_remote_async, defra_identity_new_remote_bytes,
    defra_identity_new_remote_bytes_async,
};

use callback::{failed, signature, MAX_REMOTE_SIGNATURE_LEN};
use pending::Wave;

type Signed = Result<Vec<u8>, SignFailure>;

/// How a host signs.
#[derive(Clone, Copy)]
pub(crate) enum HostCallback {
    /// Signs before it returns.
    Sync(DefraRemoteSignCallback),
    /// Starts a signature and reports it later.
    Async(DefraRemoteSignAsyncCallback),
}

/// An identity's signer: the host's callback and what it passes back.
pub(crate) struct FfiRemoteSigner {
    host: Arc<Host>,
}

struct Host {
    did: CString,
    key_type: CString,
    signer_handle: usize,
    callback: HostCallback,
}

impl FfiRemoteSigner {
    pub(crate) fn new(
        did: CString,
        key_type: CString,
        signer_handle: usize,
        callback: HostCallback,
    ) -> Self {
        Self {
            host: Arc::new(Host {
                did,
                key_type,
                signer_handle,
                callback,
            }),
        }
    }
}

impl Host {
    /// Signs `data` with the synchronous `callback`, on this thread.
    fn sign_now(&self, callback: DefraRemoteSignCallback, data: &[u8]) -> Signed {
        let mut out = vec![0u8; MAX_REMOTE_SIGNATURE_LEN];
        let mut out_len = 0usize;
        // SAFETY: the host's callback contract: the strings and the payload
        // are valid for the call, and it writes at most `out.len()` bytes to
        // `out` and their count to `out_len`.
        let status = unsafe {
            callback(
                self.signer_handle,
                self.did.as_ptr(),
                self.key_type.as_ptr(),
                data.as_ptr(),
                data.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut out_len,
            )
        };
        if status != 0 {
            return Err(failed(status, false));
        }
        signature(out_len, || {
            out.truncate(out_len);
            out
        })
    }

    /// Starts every payload with the asynchronous `callback`; completes once
    /// the host reported each, in payload order. The `Send + 'static` bound is
    /// part of what callers rely on, so it is spelled out.
    #[allow(clippy::manual_async_fn)]
    fn start(
        self: Arc<Self>,
        callback: DefraRemoteSignAsyncCallback,
        payloads: Vec<Arc<[u8]>>,
    ) -> impl Future<Output = Vec<Signed>> + Send + 'static {
        async move {
            let mut wave = Wave::new(payloads.len());
            for (index, payload) in payloads.iter().enumerate() {
                let token = wave.open(index);
                // SAFETY: the host's callback contract: the strings and the
                // payload are valid until it returns.
                let status = unsafe {
                    callback(
                        self.signer_handle,
                        self.did.as_ptr(),
                        self.key_type.as_ptr(),
                        payload.as_ptr(),
                        payload.len(),
                        token,
                    )
                };
                if status != 0 {
                    wave.fail(token, failed(status, true));
                }
            }
            wave.results().await
        }
    }
}

impl RemoteSigner for FfiRemoteSigner {
    /// The host's `signer_handle` and callback: two registrations naming
    /// both are one signer, since the host keeps that context valid until
    /// every identity handle naming it is freed.
    fn host_context(&self) -> Option<HostContext> {
        let callback = match self.host.callback {
            HostCallback::Sync(callback) => callback as usize,
            HostCallback::Async(callback) => callback as usize,
        };
        Some(HostContext {
            handle: self.host.signer_handle,
            callback,
        })
    }

    /// Signs `data` on the calling thread: the synchronous callback at once;
    /// the asynchronous one waiting for its result, at most the network
    /// runtime's deadline.
    fn sign_sync(
        &self,
        data: &[u8],
        _authorization: Option<&SigningAuthorization>,
    ) -> Result<Vec<u8>, String> {
        let callback = match self.host.callback {
            HostCallback::Sync(callback) => {
                return self.host.sign_now(callback, data).map_err(|f| f.message)
            }
            HostCallback::Async(callback) => callback,
        };
        let edge = crate::runtime::edge()
            .ok_or_else(|| crate::helpers::ERR_RUNTIME_NOT_INITIALIZED.to_string())?;
        let wave = Arc::clone(&self.host).start(callback, vec![Arc::from(data)]);
        edge.block_on(edge.timeout(edge.deadline(), wave))
            .map_err(String::from)?
            .map_err(String::from)?
            .pop()
            .unwrap_or_else(|| Err(SignFailure::refused("the signature was never reported")))
            .map_err(|failure| failure.message)
    }

    /// The asynchronous callback starts every payload at once and the wave
    /// holds no thread; the synchronous one signs the wave on the network
    /// runtime's blocking threads.
    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        _authorization: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Signed>> {
        let host = Arc::clone(&self.host);
        let callback = match host.callback {
            HostCallback::Async(callback) => return Box::pin(host.start(callback, payloads)),
            HostCallback::Sync(callback) => callback,
        };
        let count = payloads.len();
        let sign = move || {
            payloads
                .iter()
                .map(|payload| host.sign_now(callback, payload))
                .collect::<Vec<_>>()
        };
        // Without the runtime (it could not start) there are no blocking
        // threads: sign here, as a signer with only a synchronous call must.
        let Some(edge) = crate::runtime::edge() else {
            return Box::pin(std::future::ready(sign()));
        };
        let wave = edge.spawn_blocking(sign);
        Box::pin(async move {
            wave.await
                .unwrap_or_else(|error| vec![Err(SignFailure::transient(error.to_string())); count])
        })
    }
}
