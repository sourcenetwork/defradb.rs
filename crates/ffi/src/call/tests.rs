use std::ffi::CStr;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use kovan_queue::seg_queue::SegQueue;
use regolith::sync::Event;

use super::*;
use crate::types::{defra_free_string, FfiResult};

fn pool(workers: usize) -> Pool {
    Pool::start(workers::Config::unplaced(workers)).expect("the pool starts")
}

/// What an asynchronous call's callback received, reached through the
/// call's `context`.
#[derive(Default)]
struct Received {
    calls: AtomicUsize,
    results: SegQueue<Result<String, String>>,
    done: Event,
}

impl Received {
    fn leak() -> &'static Received {
        Box::leak(Box::default())
    }

    fn context(&'static self) -> usize {
        std::ptr::from_ref(self) as usize
    }

    /// The one result, once the callback ran.
    fn wait(&self) -> Result<String, String> {
        workers::block_on(self.done.wait()).unwrap();
        assert_eq!(self.calls.load(Ordering::Acquire), 1);
        self.results.pop().expect("the callback stored its result")
    }
}

unsafe extern "C" fn received(context: usize, result: FfiResult) {
    let received = &*(context as *const Received);
    let text = |ptr: *mut std::ffi::c_char| {
        let text = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        defra_free_string(ptr);
        text
    };
    received.results.push(if result.status == 0 {
        Ok(text(result.value))
    } else {
        Err(text(result.error))
    });
    received.calls.fetch_add(1, Ordering::AcqRel);
    received.done.set();
}

#[test]
fn a_synchronous_call_from_a_plain_thread_returns_its_result() {
    let pool = pool(1);
    let on_worker = run(Some(&pool), Target::LeastLoaded, || async {
        Ok::<_, String>(workers::on_worker())
    });
    assert_eq!(on_worker, Ok(true), "the work ran on a worker");
    let failed = run(Some(&pool), Target::LeastLoaded, || async {
        Err::<(), _>("refused".to_string())
    });
    assert_eq!(failed, Err("refused".to_string()));
}

/// The one worker suspends the first call's task and runs the second while
/// the first caller waits: the wait parks the caller's thread, never the
/// worker. A wait that blocked the worker would never let the second call
/// run, and the first would never finish.
#[test]
fn a_synchronous_call_parks_only_its_caller() {
    let pool = pool(1);
    let started = Arc::new(Event::new());
    let release = Arc::new(Event::new());

    let first = {
        let pool = pool.clone();
        let started = Arc::clone(&started);
        let release = Arc::clone(&release);
        std::thread::spawn(move || {
            run(Some(&pool), Target::Worker(0), move || async move {
                started.set();
                release.wait().await;
                Ok::<_, String>("first")
            })
        })
    };
    workers::block_on(started.wait()).unwrap();
    let second = run(Some(&pool), Target::Worker(0), move || async move {
        release.set();
        Ok::<_, String>("second")
    });

    assert_eq!(second, Ok("second"));
    assert_eq!(first.join().unwrap(), Ok("first"));
}

/// A synchronous call made on a worker is refused instead of stopping it.
#[test]
fn a_synchronous_call_on_a_worker_is_refused() {
    let pool = pool(1);
    let inner = pool.clone();
    let nested = run(Some(&pool), Target::Worker(0), move || async move {
        Ok::<_, String>(run(Some(&inner), Target::Worker(0), || async {
            Ok::<_, String>(())
        }))
    });
    assert_eq!(nested, Ok(Err(ERR_ON_WORKER.to_string())));
    assert_eq!(
        wait(async { Ok::<_, String>(1) }),
        Ok(1),
        "a plain thread waits"
    );
}

/// An asynchronous call as the C FFI makes one: its admission detached, its
/// task answering `reply` on the worker that ran it.
fn call_async<M, Fut>(pool: &Pool, reply: Reply, make: M)
where
    M: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<String, String>> + 'static,
{
    let pool = pool.clone();
    detach(async move {
        drop(
            pool.submit(Target::LeastLoaded, move || async move {
                let result = make().await;
                reply.send(FfiResult::from(result));
            })
            .await,
        );
    });
}

#[test]
fn an_asynchronous_call_completes_through_its_callback() {
    let pool = pool(1);
    let ok = Received::leak();
    call_async(&pool, Reply::new(received, ok.context()), || async {
        Ok(r#"{"answer":42}"#.to_string())
    });
    assert_eq!(ok.wait(), Ok(r#"{"answer":42}"#.to_string()));

    let failed = Received::leak();
    call_async(&pool, Reply::new(received, failed.context()), || async {
        Err("refused".to_string())
    });
    assert_eq!(failed.wait(), Err("refused".to_string()));
}

/// The callback runs once even when the call ends without a result: its
/// task panicked, or the pool was shut down before it ran.
#[test]
fn an_asynchronous_call_that_ends_without_a_result_still_reports_once() {
    let pool = pool(1);
    let panicked = Received::leak();
    call_async(&pool, Reply::new(received, panicked.context()), || async {
        if workers::on_worker() {
            panic!("the call failed");
        }
        Ok(String::new())
    });
    assert_eq!(panicked.wait(), Err(reply::ERR_ENDED.to_string()));

    pool.shutdown();
    let refused = Received::leak();
    call_async(&pool, Reply::new(received, refused.context()), || async {
        Ok(String::new())
    });
    assert_eq!(refused.wait(), Err(reply::ERR_ENDED.to_string()));
}

/// An asynchronous call made on a worker (from another call's callback) is
/// admitted without blocking it, and completes through its own callback.
#[test]
fn an_asynchronous_call_made_on_a_worker_is_admitted_without_blocking_it() {
    let pool = pool(1);
    let chained = Received::leak();
    let inner = pool.clone();
    let outer = run(Some(&pool), Target::Worker(0), move || async move {
        call_async(&inner, Reply::new(received, chained.context()), || async {
            Ok("chained".to_string())
        });
        Ok::<_, String>(())
    });
    assert_eq!(outer, Ok(()));
    assert_eq!(chained.wait(), Ok("chained".to_string()));
}

/// A node with a `Note` collection.
fn node() -> usize {
    assert!(crate::runtime::init_runtime());
    let result = crate::node::new_node(crate::types::NodeInitOptions::default());
    assert_eq!(result.status, 0);
    let sdl = std::ffi::CString::new("type Note { text: String }").unwrap();
    let schema =
        unsafe { crate::schema::add_schema(result.node_ptr, std::ptr::null(), sdl.as_ptr()) };
    assert_eq!(schema.status, 0);
    unsafe { defra_free_string(schema.value) };
    result.node_ptr
}

/// A node's data-path calls run on its worker pool, not the caller's thread.
#[test]
fn a_node_call_runs_on_the_nodes_pool() {
    let node = node();
    assert_eq!(
        on_node(node, || async { Ok::<_, String>(workers::on_worker()) }),
        Ok(true)
    );
    assert!(
        on_node(0, || async { Ok::<_, String>(()) }).is_err(),
        "no node 0"
    );
    crate::node::node_close(node);
}

/// An asynchronous GraphQL request completes through its callback, called on
/// the worker that ran it, with what the synchronous form returns.
#[test]
fn an_asynchronous_request_completes_through_its_callback() {
    let node = node();
    let created = Received::leak();
    let mutation =
        std::ffi::CString::new(r#"mutation { add_Note(input: {text: "async"}) { text } }"#)
            .unwrap();
    unsafe {
        crate::query::exec_request_async(
            node,
            std::ptr::null(),
            std::ptr::null(),
            mutation.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            -1,
            received,
            created.context(),
        )
    };
    let response: serde_json::Value = serde_json::from_str(&created.wait().unwrap()).unwrap();
    assert_eq!(response["data"]["add_Note"][0]["text"], "async");

    let refused = Received::leak();
    unsafe {
        crate::query::exec_request_async(
            node,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            -1,
            received,
            refused.context(),
        )
    };
    assert_eq!(refused.wait(), Err("request_query is null".to_string()));
    crate::node::node_close(node);
}

/// A synchronous call made on one of the node's workers is refused with the
/// reason, instead of stopping the worker it would wait on.
#[test]
fn a_synchronous_request_made_on_a_worker_is_refused() {
    let node = node();
    let refused = on_node(node, move || async move {
        let query = std::ffi::CString::new("{ Note { text } }").unwrap();
        let result = unsafe {
            crate::query::exec_request(
                node,
                std::ptr::null(),
                std::ptr::null(),
                query.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(result.status, 1);
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy().into_owned() };
        unsafe { defra_free_string(result.error) };
        Ok::<_, String>(error)
    });
    assert_eq!(refused, Ok(ERR_ON_WORKER.to_string()));
    crate::node::node_close(node);
}

/// An asynchronous request made on a worker (a callback that calls back in)
/// is checked and admitted as a task of that worker, which it never blocks.
#[test]
fn an_asynchronous_request_made_on_a_worker_completes() {
    let node = node();
    let read = Received::leak();
    let made = on_node(node, move || async move {
        let query = std::ffi::CString::new("{ Note { text } }").unwrap();
        unsafe {
            crate::query::exec_request_async(
                node,
                std::ptr::null(),
                std::ptr::null(),
                query.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                -1,
                received,
                read.context(),
            )
        };
        Ok::<_, String>(())
    });
    assert_eq!(made, Ok(()));
    let response: serde_json::Value = serde_json::from_str(&read.wait().unwrap()).unwrap();
    assert!(response["data"]["Note"].is_array(), "{response}");
    crate::node::node_close(node);
}
