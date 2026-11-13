//! Calling the C API from tests as a C caller would: handles, C strings,
//! and results whose strings are freed once read.

use std::ffi::{CStr, CString};
use std::ptr;
use std::time::{Duration, Instant};

use crate::types::{
    defra_new_node_result_free, defra_result_free, CollectionOptions, DefraResult, NodeInitOptions,
};

/// The longest pause between two tries.
const MAX_PAUSE: Duration = Duration::from_millis(50);

/// A point in time a test stops waiting at, pausing between tries as a C
/// caller polling the API would: briefly at first, so a ready answer is seen
/// at once, then longer, up to [`MAX_PAUSE`].
pub(crate) struct Deadline {
    end: Instant,
    pause: Duration,
}

impl Deadline {
    /// A deadline `timeout` from now.
    pub(crate) fn after(timeout: Duration) -> Self {
        Self {
            end: Instant::now() + timeout,
            pause: Duration::from_millis(1),
        }
    }

    /// Pauses before the next try, never past the deadline; `false` once it
    /// has passed.
    pub(crate) fn pause(&mut self) -> bool {
        let Some(left) = self.end.checked_duration_since(Instant::now()) else {
            return false;
        };
        std::thread::sleep(self.pause.min(left));
        self.pause = (self.pause * 2).min(MAX_PAUSE);
        true
    }
}

/// Tries `attempt` until it answers `Ok`, failing with its last `Err` once
/// `timeout` has passed.
pub(crate) fn wait_for<T, A>(timeout: Duration, mut attempt: A) -> T
where
    A: FnMut() -> Result<T, String>,
{
    let mut deadline = Deadline::after(timeout);
    loop {
        match attempt() {
            Ok(value) => return value,
            Err(why) => assert!(deadline.pause(), "gave up after {timeout:?}: {why}"),
        }
    }
}

/// A C string for `value`.
pub(crate) fn cstr(value: &str) -> CString {
    CString::new(value).expect("test strings hold no NUL")
}

/// Reads and frees `result`: its value on status 0 or 2, its error on 1.
pub(crate) fn take(mut result: DefraResult) -> (i32, Result<String, String>) {
    let value = text(result.value);
    let error = text(result.error);
    unsafe { defra_result_free(&mut result) };
    let outcome = if result.status == 1 {
        Err(error)
    } else {
        Ok(value)
    };
    (result.status, outcome)
}

/// The text of a C string a result holds, empty for null.
pub(crate) fn text(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// The value of a call that must succeed.
pub(crate) fn ok(result: DefraResult) -> String {
    match take(result) {
        (_, Ok(value)) => value,
        (_, Err(error)) => panic!("call failed: {error}"),
    }
}

/// The error of a call that must fail.
pub(crate) fn err(result: DefraResult) -> String {
    match take(result) {
        (_, Err(error)) => error,
        (_, Ok(value)) => panic!("call succeeded with {value:?}"),
    }
}

/// The JSON value of a call that must succeed.
pub(crate) fn ok_json(result: DefraResult) -> serde_json::Value {
    let value = ok(result);
    serde_json::from_str(&value).unwrap_or_else(|e| panic!("not JSON ({e}): {value}"))
}

/// Options for an in-memory node without P2P or block signing.
pub(crate) fn local_options() -> NodeInitOptions {
    NodeInitOptions {
        disable_p2p: 1,
        disable_signing: 1,
        ..NodeInitOptions::default()
    }
}

/// A node built from `options`.
pub(crate) fn node_with(options: NodeInitOptions) -> usize {
    let mut result = unsafe { crate::node::defra_node_new(options) };
    if result.status != 0 {
        let error = text(result.error);
        unsafe { defra_new_node_result_free(&mut result) };
        panic!("defra_node_new failed: {error}");
    }
    result.node_ptr
}

/// An in-memory node without P2P.
pub(crate) fn node() -> usize {
    node_with(local_options())
}

/// Closes `node`.
pub(crate) fn close(node: usize) {
    ok(crate::node::defra_node_close(node));
}

/// Adds the collections `sdl` defines to `node`.
pub(crate) fn add_collection(node: usize, sdl: &str) -> serde_json::Value {
    let sdl = cstr(sdl);
    ok_json(unsafe { crate::collection::defra_collection_add(node, sdl.as_ptr(), 0) })
}

/// Runs the GraphQL request `request` on `handle` as `identity`, answering
/// with its GraphQL result.
pub(crate) fn query_as(handle: usize, request: &str, identity: usize) -> serde_json::Value {
    let request = cstr(request);
    ok_json(unsafe {
        crate::query::defra_query_execute(
            handle,
            request.as_ptr(),
            identity,
            ptr::null(),
            ptr::null(),
        )
    })
}

/// Runs the GraphQL request `request` on `handle` without an identity.
pub(crate) fn query(handle: usize, request: &str) -> serde_json::Value {
    query_as(handle, request, 0)
}

/// Collection options selecting the collection `name`.
pub(crate) struct Named(CString);

impl Named {
    pub(crate) fn new(name: &str) -> Self {
        Self(cstr(name))
    }

    pub(crate) fn options(&self) -> CollectionOptions {
        CollectionOptions {
            name: self.0.as_ptr(),
            ..CollectionOptions::default()
        }
    }
}
