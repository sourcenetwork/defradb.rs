use std::ffi::{c_char, c_int, CStr, CString};
use std::future::Future;
use std::pin::pin;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crypto::{Key, PrivateKey, PublicKey};
use identity::Identity;
use kovan_queue::seg_queue::SegQueue;
use serde_json::Value;

use super::*;
use crate::types::{defra_free_string, FfiResult, NodeInitOptions};

/// A host that keeps an Ed25519 key; tests reach it through the opaque
/// `signer_handle`, so each test has its own and none share state.
struct TestHost {
    key: Vec<u8>,
    did: String,
    public_key: Vec<u8>,
    /// Answer inside the callback instead of queuing the request.
    inline: bool,
    /// What the callback returns for every request it is asked to start.
    start_status: c_int,
    calls: AtomicUsize,
    threads: SegQueue<std::thread::ThreadId>,
    started: SegQueue<(usize, Vec<u8>)>,
}

impl TestHost {
    fn new(inline: bool, start_status: c_int) -> &'static TestHost {
        let key = crypto::generate_ed25519().unwrap();
        let raw = key.raw().to_vec();
        let identity = identity::RawIdentity::from_ed25519(key).unwrap();
        Box::leak(Box::new(TestHost {
            key: raw,
            did: identity.did().unwrap().to_string(),
            public_key: identity.public_key_bytes(),
            inline,
            start_status,
            calls: AtomicUsize::new(0),
            threads: SegQueue::new(),
            started: SegQueue::new(),
        }))
    }

    fn handle(&'static self) -> usize {
        std::ptr::from_ref(self) as usize
    }

    fn sign(&self, payload: &[u8]) -> Vec<u8> {
        crypto::Ed25519PrivateKey::from_bytes(&self.key)
            .unwrap()
            .sign(payload)
            .unwrap()
    }

    fn verifies(&self, payload: &[u8], signature: &[u8]) -> bool {
        crypto::Ed25519PublicKey::from_bytes(&self.public_key)
            .unwrap()
            .verify(payload, signature)
            .unwrap()
    }

    /// Reports the signature of `payload` under `completion`.
    fn answer(&self, completion: usize, payload: &[u8]) -> c_int {
        let signature = self.sign(payload);
        defra_remote_sign_complete(completion, 0, signature.as_ptr(), signature.len())
    }

    /// Registers this host's key with `register`, and returns its signer.
    fn register<R>(&'static self, register: R) -> Arc<dyn RemoteSigner>
    where
        R: FnOnce(*const c_char, *const u8, usize, *const c_char, usize) -> FfiResult,
    {
        let did = CString::new(self.did.clone()).unwrap();
        let key_type = CString::new("ed25519").unwrap();
        let result = register(
            did.as_ptr(),
            self.public_key.as_ptr(),
            self.public_key.len(),
            key_type.as_ptr(),
            self.handle(),
        );
        assert_eq!(result.status, 0, "registration should succeed");
        unsafe { defra_free_string(result.value) };
        defra_core::signing::get_identity(&self.did)
            .and_then(|config| config.remote_signer.clone())
            .expect("the identity is stored with its signer")
    }

    fn register_async(&'static self) -> Arc<dyn RemoteSigner> {
        self.register(|did, key, len, key_type, handle| {
            register_remote_identity_bytes_async(did, key, len, key_type, handle, async_callback)
        })
    }

    fn register_sync(&'static self) -> Arc<dyn RemoteSigner> {
        self.register(|did, key, len, key_type, handle| {
            register_remote_identity_bytes(did, key, len, key_type, handle, sync_callback)
        })
    }
}

unsafe extern "C" fn async_callback(
    signer_handle: usize,
    _did: *const c_char,
    _key_type: *const c_char,
    payload_ptr: *const u8,
    payload_len: usize,
    completion: usize,
) -> c_int {
    let host = &*(signer_handle as *const TestHost);
    host.calls.fetch_add(1, Ordering::Relaxed);
    if host.start_status != 0 {
        return host.start_status;
    }
    let payload = std::slice::from_raw_parts(payload_ptr, payload_len).to_vec();
    if host.inline {
        assert_eq!(host.answer(completion, &payload), 0);
    } else {
        host.started.push((completion, payload));
    }
    0
}

unsafe extern "C" fn sync_callback(
    signer_handle: usize,
    _did: *const c_char,
    _key_type: *const c_char,
    payload_ptr: *const u8,
    payload_len: usize,
    out_signature_ptr: *mut u8,
    out_signature_capacity: usize,
    out_signature_len: *mut usize,
) -> c_int {
    let host = &*(signer_handle as *const TestHost);
    host.calls.fetch_add(1, Ordering::Relaxed);
    host.threads.push(std::thread::current().id());
    let signature = host.sign(std::slice::from_raw_parts(payload_ptr, payload_len));
    if signature.len() > out_signature_capacity {
        return 1;
    }
    ptr::copy_nonoverlapping(signature.as_ptr(), out_signature_ptr, signature.len());
    *out_signature_len = signature.len();
    0
}

fn payloads(count: usize) -> Vec<Arc<[u8]>> {
    (0..count)
        .map(|index| Arc::from(format!("payload-{index}").into_bytes()))
        .collect()
}

fn poll_once<F>(future: std::pin::Pin<&mut F>) -> Poll<F::Output>
where
    F: Future,
{
    future.poll(&mut Context::from_waker(Waker::noop()))
}

fn edge() -> &'static edge::Edge {
    assert!(crate::runtime::init_runtime());
    crate::runtime::edge().expect("the runtime started")
}

/// The callbacks return before any signature exists: the wave is pending,
/// holding no thread, and completes once the host answers, from another
/// thread and in any order, with each result in its payload's place.
#[test]
fn an_async_wave_holds_no_thread_and_completes_when_the_host_answers() {
    let host = TestHost::new(false, 0);
    let signer = host.register_async();
    let payloads = payloads(3);

    let mut wave = pin!(signer.sign_batch(payloads.clone(), None));
    assert!(poll_once(wave.as_mut()).is_pending());
    assert_eq!(host.calls.load(Ordering::Relaxed), 3);

    let answering = std::thread::spawn(move || {
        let mut started = Vec::new();
        while let Some(request) = host.started.pop() {
            started.push(request);
        }
        assert_eq!(started.len(), 3);
        for (completion, payload) in started.into_iter().rev() {
            assert_eq!(host.answer(completion, &payload), 0);
        }
    });
    answering.join().unwrap();

    let Poll::Ready(signatures) = poll_once(wave.as_mut()) else {
        panic!("every result is in, so the wave is done");
    };
    assert_eq!(signatures.len(), 3);
    for (payload, signature) in payloads.iter().zip(signatures) {
        assert!(host.verifies(payload, &signature.unwrap()));
    }
}

/// A result lands once: a second report of the same request, and a report
/// of a request nobody opened, are refused.
#[test]
fn a_completion_lands_once() {
    let host = TestHost::new(false, 0);
    let signer = host.register_async();

    let mut wave = pin!(signer.sign_batch(payloads(1), None));
    assert!(poll_once(wave.as_mut()).is_pending());
    let (completion, payload) = host.started.pop().unwrap();
    assert_eq!(host.answer(completion, &payload), 0);
    assert_eq!(host.answer(completion, &payload), 1);
    assert_eq!(defra_remote_sign_complete(0, 0, ptr::null(), 0), 1);

    let Poll::Ready(signatures) = poll_once(wave.as_mut()) else {
        panic!("the wave's one result is in");
    };
    assert!(host.verifies(&payload, signatures[0].as_ref().unwrap()));
}

/// A wave dropped before the host answered (its deadline passed) withdraws
/// its requests: a late report finds nothing waiting.
#[test]
fn a_wave_that_stops_waiting_withdraws_its_requests() {
    let host = TestHost::new(false, 0);
    let signer = host.register_async();

    {
        let mut wave = pin!(signer.sign_batch(payloads(2), None));
        assert!(poll_once(wave.as_mut()).is_pending());
    }

    let mut late = 0;
    while let Some((completion, payload)) = host.started.pop() {
        assert_eq!(host.answer(completion, &payload), 1);
        late += 1;
    }
    assert_eq!(late, 2);
}

/// What the host reports decides each request's failure: a request it could
/// not start, a retryable status, a refusal and an empty signature.
#[test]
fn host_statuses_fail_only_their_own_request() {
    let refusing = TestHost::new(false, 7);
    let signer = refusing.register_async();
    let mut wave = pin!(signer.sign_batch(payloads(2), None));
    let Poll::Ready(signatures) = poll_once(wave.as_mut()) else {
        panic!("requests that never started fail at once");
    };
    for failure in signatures {
        let failure = failure.unwrap_err();
        assert!(!failure.transient);
        assert_eq!(
            failure.message,
            "remote signer callback failed with status 7"
        );
    }

    let host = TestHost::new(false, 0);
    let signer = host.register_async();
    let payloads = payloads(4);
    let mut wave = pin!(signer.sign_batch(payloads.clone(), None));
    assert!(poll_once(wave.as_mut()).is_pending());
    let mut started = Vec::new();
    while let Some(request) = host.started.pop() {
        started.push(request);
    }
    started.sort_by_key(|(completion, _)| *completion);
    let [(signed, payload), (retryable, _), (refused, _), (empty, _)] = started.as_slice() else {
        panic!("four requests were started");
    };
    assert_eq!(host.answer(*signed, payload), 0);
    assert_eq!(
        defra_remote_sign_complete(*retryable, DEFRA_SIGN_RETRYABLE, ptr::null(), 0),
        0
    );
    assert_eq!(defra_remote_sign_complete(*refused, 9, ptr::null(), 0), 0);
    assert_eq!(defra_remote_sign_complete(*empty, 0, ptr::null(), 0), 0);

    let Poll::Ready(signatures) = poll_once(wave.as_mut()) else {
        panic!("every request was reported");
    };
    assert!(host.verifies(&payloads[0], signatures[0].as_ref().unwrap()));
    assert!(signatures[1].as_ref().unwrap_err().transient);
    let refused = signatures[2].as_ref().unwrap_err();
    assert!(!refused.transient);
    assert_eq!(
        refused.message,
        "remote signer callback failed with status 9"
    );
    assert_eq!(
        signatures[3].as_ref().unwrap_err().message,
        "remote signer callback returned an empty signature"
    );
}

/// The synchronous form of an asynchronous signer waits on the calling
/// thread for the host's answer.
#[test]
fn an_async_signer_signs_synchronously_from_a_plain_thread() {
    edge();
    let host = TestHost::new(true, 0);
    let signer = host.register_async();

    let signature = signer.sign_sync(b"plain-thread", None).unwrap();
    assert!(host.verifies(b"plain-thread", &signature));
    assert_eq!(host.calls.load(Ordering::Relaxed), 1);
}

/// A wave signed through the synchronous callback runs it on the network
/// runtime's blocking threads, never on the thread that awaits the wave.
#[test]
fn a_synchronous_callback_signs_a_wave_on_the_blocking_threads() {
    let edge = edge();
    let host = TestHost::new(false, 0);
    let signer = host.register_sync();
    let payloads = payloads(2);

    let signatures = edge
        .block_on(signer.sign_batch(payloads.clone(), None))
        .unwrap();
    for (payload, signature) in payloads.iter().zip(signatures) {
        assert!(host.verifies(payload, &signature.unwrap()));
    }
    let awaiting = std::thread::current().id();
    let mut calls = 0;
    while let Some(thread) = host.threads.pop() {
        assert_ne!(thread, awaiting);
        calls += 1;
    }
    assert_eq!(calls, 2);

    // The synchronous form still signs on the calling thread.
    let signature = signer.sign_sync(b"inline", None).unwrap();
    assert!(host.verifies(b"inline", &signature));
    assert_eq!(host.threads.pop(), Some(awaiting));
}

/// A document written on a signing node through a host's asynchronous
/// callback carries that host's signatures.
#[test]
fn a_write_signs_its_blocks_through_the_async_callback() {
    edge();
    let host = TestHost::new(true, 0);
    host.register_async();

    let result = crate::node::new_node(NodeInitOptions {
        enable_signing: 1,
        ..NodeInitOptions::default()
    });
    assert_eq!(result.status, 0);
    let node = result.node_ptr;
    let sdl = CString::new("type Note { text: String }").unwrap();
    let result = unsafe { crate::schema::add_schema(node, ptr::null(), sdl.as_ptr()) };
    assert_eq!(result.status, 0);
    unsafe { defra_free_string(result.value) };

    let did = CString::new(host.did.clone()).unwrap();
    let exec = |query: &str| -> Value {
        let query = CString::new(query).unwrap();
        let result = unsafe {
            crate::query::exec_request_with_signing(
                node,
                std::ptr::null(),
                did.as_ptr(),
                query.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                1,
            )
        };
        assert_eq!(result.status, 0, "the request should succeed");
        let json = unsafe { CStr::from_ptr(result.value).to_string_lossy().into_owned() };
        unsafe { defra_free_string(result.value) };
        let response: Value = serde_json::from_str(&json).unwrap();
        assert!(
            response.get("errors").is_none_or(Value::is_null),
            "request returned errors: {response}"
        );
        response
    };

    let created = exec(r#"mutation { add_Note(input: {text: "signed"}) { _docID } }"#);
    let doc_id = created
        .pointer("/data/add_Note/0/_docID")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let commits = exec(&format!(
        r#"query {{ _commits(docID: "{doc_id}") {{ signature {{ identity }} }} }}"#
    ));
    let identities: Vec<&str> = commits
        .pointer("/data/_commits")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(|commit| {
            commit
                .pointer("/signature/identity")
                .and_then(Value::as_str)
        })
        .collect();

    assert!(
        !identities.is_empty(),
        "the write's blocks are signed: {commits}"
    );
    let public_key_hex = hex::encode(&host.public_key);
    assert!(identities
        .iter()
        .all(|identity| *identity == public_key_hex));
    assert!(host.calls.load(Ordering::Relaxed) > 0);

    crate::node::node_close(node);
}
