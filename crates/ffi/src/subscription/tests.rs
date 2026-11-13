//! GraphQL subscriptions through the C API: starting one, what it yields,
//! and how it ends.

use std::ffi::CString;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use super::{defra_subscription_close, defra_subscription_poll};
use crate::query::defra_query_execute;
use crate::test_support::{
    add_collection, close, cstr, err, node, ok, query, take, wait_for, Deadline,
};

/// Starts the subscription `request` on `node` and answers with its ID.
fn subscribe(node: usize, request: &str) -> CString {
    let request = cstr(request);
    let (status, id) =
        take(unsafe { defra_query_execute(node, request.as_ptr(), 0, ptr::null(), ptr::null()) });
    assert_eq!(status, 2, "a subscription answers with status 2");
    cstr(&id.expect("a subscription answers with its ID"))
}

/// One poll: `Some(result)` when one was pending, `None` when none was.
fn poll(id: &CString) -> Option<Value> {
    match take(unsafe { defra_subscription_poll(id.as_ptr()) }) {
        (0, Ok(json)) => Some(serde_json::from_str(&json).expect("a result is JSON")),
        (2, _) => None,
        (status, outcome) => panic!("poll failed with status {status}: {outcome:?}"),
    }
}

/// Polls until a result arrives, or fails after 10 seconds.
fn next(id: &CString) -> Value {
    wait_for(Duration::from_secs(10), || {
        poll(id).ok_or_else(|| "no result arrived".to_string())
    })
}

#[test]
fn a_subscription_needs_a_node() {
    let request = cstr("subscription { User { name } }");
    let (status, _) =
        take(unsafe { defra_query_execute(0, request.as_ptr(), 0, ptr::null(), ptr::null()) });
    assert_eq!(status, 1);
}

#[test]
fn closing_an_unknown_subscription_does_nothing_and_polling_one_fails() {
    let unknown = cstr("999999");
    ok(unsafe { defra_subscription_close(unknown.as_ptr()) });
    let error = err(unsafe { defra_subscription_poll(unknown.as_ptr()) });
    assert!(error.contains("invalid subscription ID"), "{error}");
    let not_a_number = cstr("not-an-id");
    err(unsafe { defra_subscription_poll(not_a_number.as_ptr()) });
}

#[test]
fn closing_the_node_ends_its_subscriptions() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let first = subscribe(node, "subscription { User { name } }");
    let second = subscribe(node, "subscription { User { name } }");

    close(node);
    err(unsafe { defra_subscription_poll(first.as_ptr()) });
    err(unsafe { defra_subscription_poll(second.as_ptr()) });
}

#[test]
fn a_subscription_yields_only_its_own_collection() {
    let node = node();
    add_collection(node, "type Author { name: String }");
    add_collection(node, "type Article { title: String }");
    let id = subscribe(node, "subscription { Author { name } }");

    query(
        node,
        r#"mutation { add_Article(input: {title: "Unrelated"}) { _docID } }"#,
    );
    query(
        node,
        r#"mutation { add_Author(input: {name: "Bob"}) { _docID } }"#,
    );

    let result = next(&id);
    assert_eq!(result["data"]["Author"][0]["name"], "Bob", "{result}");
    assert_eq!(poll(&id), None, "the Article add yields nothing");

    ok(unsafe { defra_subscription_close(id.as_ptr()) });
    close(node);
}

/// Polls of one ID from several threads take no lock: each result goes to
/// exactly one poller, and none is lost or repeated.
#[test]
fn concurrent_polls_of_one_subscription_deliver_each_result_once() {
    const WRITES: usize = 32;
    const POLLERS: usize = 4;

    let node = node();
    add_collection(node, "type Counter { value: Int }");
    let id = Arc::new(subscribe(node, "subscription { Counter { value } }"));
    for value in 0..WRITES {
        query(
            node,
            &format!("mutation {{ add_Counter(input: {{value: {value}}}) {{ _docID }} }}"),
        );
    }

    let received = Arc::new(AtomicUsize::new(0));
    let pollers: Vec<_> = (0..POLLERS)
        .map(|_| {
            let id = Arc::clone(&id);
            let received = Arc::clone(&received);
            std::thread::spawn(move || {
                let mut deadline = Deadline::after(Duration::from_secs(20));
                let mut values = Vec::new();
                while received.load(Ordering::SeqCst) < WRITES {
                    match poll(&id) {
                        Some(result) => {
                            received.fetch_add(1, Ordering::SeqCst);
                            values.push(
                                result["data"]["Counter"][0]["value"]
                                    .as_u64()
                                    .unwrap_or_else(|| panic!("no value in {result}")),
                            );
                        }
                        None if deadline.pause() => {}
                        None => break,
                    }
                }
                values
            })
        })
        .collect();

    let mut values: Vec<u64> = pollers
        .into_iter()
        .flat_map(|poller| poller.join().expect("a poller panicked"))
        .collect();
    values.sort_unstable();
    assert_eq!(values, (0..WRITES as u64).collect::<Vec<_>>());

    ok(unsafe { defra_subscription_close(id.as_ptr()) });
    close(node);
}

#[test]
fn subscriptions_opened_and_closed_in_quick_succession_get_their_own_ids() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let mut ids = Vec::new();
    for _ in 0..50 {
        let id = subscribe(node, "subscription { User { name } }");
        assert_eq!(poll(&id), None);
        ok(unsafe { defra_subscription_close(id.as_ptr()) });
        err(unsafe { defra_subscription_poll(id.as_ptr()) });
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 50, "every subscription has its own ID");
    close(node);
}

#[test]
fn polling_and_closing_one_subscription_from_several_threads_is_safe() {
    let node = node();
    add_collection(node, "type User { name: String }");
    let id = Arc::new(subscribe(node, "subscription { User { name } }"));

    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let id = Arc::clone(&id);
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let (status, _) = take(unsafe { defra_subscription_poll(id.as_ptr()) });
                    assert!(matches!(status, 1 | 2), "poll status {status}");
                }
                if thread == 0 {
                    ok(unsafe { defra_subscription_close(id.as_ptr()) });
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a thread panicked");
    }

    err(unsafe { defra_subscription_poll(id.as_ptr()) });
    close(node);
}
