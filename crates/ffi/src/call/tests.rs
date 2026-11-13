use std::ffi::CStr;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use kovan_queue::seg_queue::SegQueue;
use regolith::sync::Event;

use super::*;
use crate::types::{defra_result_free, DefraResult};

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

    /// Whether the callback has been called yet, without waiting for it.
    fn arrived(&self) -> bool {
        self.calls.load(Ordering::Acquire) > 0
    }

    /// The one result, once the callback ran.
    fn wait(&self) -> Result<String, String> {
        workers::block_on(self.done.wait()).unwrap();
        assert_eq!(self.calls.load(Ordering::Acquire), 1);
        self.results.pop().expect("the callback stored its result")
    }
}

unsafe extern "C" fn received(context: usize, mut result: DefraResult) {
    let received = &*(context as *const Received);
    let outcome = if result.status == 0 {
        Ok(CStr::from_ptr(result.value).to_string_lossy().into_owned())
    } else {
        Err(CStr::from_ptr(result.error).to_string_lossy().into_owned())
    };
    defra_result_free(&mut result);
    received.results.push(outcome);
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
                reply.send(DefraResult::from(result));
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
    let node = crate::test_support::node();
    crate::test_support::add_collection(node, "type Note { text: String }");
    node
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
    crate::test_support::close(node);
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
        crate::query::defra_query_execute_async(
            node,
            mutation.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            received,
            created.context(),
        )
    };
    let response: serde_json::Value = serde_json::from_str(&created.wait().unwrap()).unwrap();
    assert_eq!(response["data"]["add_Note"][0]["text"], "async");

    // Refused before it runs: the callback still answers, once.
    let refused = Received::leak();
    unsafe {
        crate::query::defra_query_execute_async(
            0,
            mutation.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            received,
            refused.context(),
        )
    };
    assert_eq!(
        refused.wait(),
        Err(crate::target::ERR_INVALID_HANDLE.to_string())
    );
    crate::test_support::close(node);
}

/// A synchronous call made on one of the node's workers is refused with the
/// reason, instead of stopping the worker it would wait on.
#[test]
fn a_synchronous_request_made_on_a_worker_is_refused() {
    let node = node();
    let refused = on_node(node, move || async move {
        let query = std::ffi::CString::new("{ Note { text } }").unwrap();
        let mut result = unsafe {
            crate::query::defra_query_execute(
                node,
                query.as_ptr(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(result.status, 1);
        let error = unsafe { CStr::from_ptr(result.error).to_string_lossy().into_owned() };
        unsafe { defra_result_free(&mut result) };
        Ok::<_, String>(error)
    });
    assert_eq!(refused, Ok(ERR_ON_WORKER.to_string()));
    crate::test_support::close(node);
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
            crate::query::defra_query_execute_async(
                node,
                query.as_ptr(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                received,
                read.context(),
            )
        };
        Ok::<_, String>(())
    });
    assert_eq!(made, Ok(()));
    let response: serde_json::Value = serde_json::from_str(&read.wait().unwrap()).unwrap();
    assert!(response["data"]["Note"].is_array(), "{response}");
    crate::test_support::close(node);
}

/// A transaction on `node`.
fn transaction(node: usize) -> usize {
    let created = crate::txn::defra_transaction_create(node, 0);
    assert_eq!(created.status, 0, "the transaction begins");
    created.txn_ptr
}

/// Queues the GraphQL request `query` on `handle` asynchronously, answering
/// `into`.
fn execute_async(handle: usize, query: &str, into: &'static Received) {
    let query = std::ffi::CString::new(query).unwrap();
    unsafe {
        crate::query::defra_query_execute_async(
            handle,
            query.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            received,
            into.context(),
        )
    };
}

/// `proofs/tla/TxnTurns.tla`: an asynchronous query on a transaction returns
/// once it holds its turn, before it runs. While an earlier call still holds
/// the transaction its callback has not run; it runs once that call ends.
#[test]
fn an_async_query_in_a_transaction_returns_before_it_runs() {
    let node = node();
    let txn = transaction(node);
    let earlier = crate::target::resolve_target(txn).unwrap_or_else(|_| panic!("a txn"));
    workers::block_on(earlier.turn()).unwrap();

    let created = Received::leak();
    execute_async(
        txn,
        r#"mutation { add_Note(input: {text: "queued"}) { text } }"#,
        created,
    );
    assert!(
        !created.arrived(),
        "it returned while the earlier call still holds the transaction"
    );

    drop(earlier);
    let response: serde_json::Value = serde_json::from_str(&created.wait().unwrap()).unwrap();
    assert_eq!(
        response["data"]["add_Note"][0]["text"], "queued",
        "{response}"
    );
    let mut committed = crate::txn::defra_transaction_commit(txn);
    assert_eq!(committed.status, 0);
    unsafe { defra_result_free(&mut committed) };
    crate::test_support::close(node);
}

/// INV_NoOvertake: calls on a transaction run in the order they were made. A
/// read queued after an asynchronous write sees it, and a commit made after
/// both waits for them, so the write is committed.
#[test]
fn calls_on_a_transaction_run_in_the_order_they_were_made() {
    let node = node();
    let txn = transaction(node);
    let earlier = crate::target::resolve_target(txn).unwrap_or_else(|_| panic!("a txn"));
    workers::block_on(earlier.turn()).unwrap();

    let written = Received::leak();
    let read = Received::leak();
    execute_async(
        txn,
        r#"mutation { add_Note(input: {text: "first"}) { text } }"#,
        written,
    );
    execute_async(txn, "{ Note { text } }", read);
    let commit = std::thread::spawn(move || {
        let mut result = crate::txn::defra_transaction_commit(txn);
        let status = result.status;
        unsafe { defra_result_free(&mut result) };
        status
    });

    drop(earlier);
    assert!(written.wait().is_ok());
    let seen: serde_json::Value = serde_json::from_str(&read.wait().unwrap()).unwrap();
    assert_eq!(seen["data"]["Note"][0]["text"], "first", "{seen}");
    assert_eq!(commit.join().unwrap(), 0, "the commit ran after both");

    let committed = crate::test_support::query(node, "{ Note { text } }");
    assert_eq!(committed["data"]["Note"][0]["text"], "first", "{committed}");
    crate::test_support::close(node);
}
